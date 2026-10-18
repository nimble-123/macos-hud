//! Codex usage from `~/.codex/sessions/**/*.jsonl` and the `*.jsonl.zst`
//! archives Codex makes of older sessions.
//!
//! Only usage metadata is parsed; prompt and answer content is never kept,
//! and nothing outside the session folders is opened.

use std::io::Read;
use std::path::{Path, PathBuf};

use futures::future::BoxFuture;
use hud_application::ports::usage::{Cursors, FileCursor, UsageBatch, UsageError, UsageSource};
use hud_domain::usage::{
    Geo, ModelId, PlanUsage, PlanWindow, Provider, Speed, TokenUsage, ToolCall, UsageRecord,
};
use jiff::Timestamp;
use serde::Deserialize;

/// Reads the session logs of the Codex CLI, IDE extension and desktop app.
#[derive(Debug, Clone)]
pub struct CodexLogs {
    roots: Vec<PathBuf>,
}

impl CodexLogs {
    /// Logs below `roots`, usually the result of [`Self::default_roots`].
    #[must_use]
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    /// `sessions` and `archived_sessions` in `$CODEX_HOME`, else in `~/.codex`.
    #[must_use]
    pub fn default_roots(home: &Path, codex_home: Option<&Path>) -> Vec<PathBuf> {
        let base = codex_home
            .filter(|dir| dir.is_absolute())
            .map_or_else(|| home.join(".codex"), Path::to_path_buf);
        vec![base.join("sessions"), base.join("archived_sessions")]
    }
}

impl UsageSource for CodexLogs {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn read(&self, cursors: Cursors) -> BoxFuture<'_, Result<UsageBatch, UsageError>> {
        let roots = self.roots.clone();
        Box::pin(async move {
            // Plain file I/O, decompression and parsing.
            tokio::task::spawn_blocking(move || read_all(&roots, cursors))
                .await
                .map_err(|error| UsageError::Io(error.to_string()))?
        })
    }
}

fn read_all(roots: &[PathBuf], mut cursors: Cursors) -> Result<UsageBatch, UsageError> {
    let existing: Vec<&PathBuf> = roots.iter().filter(|root| root.is_dir()).collect();
    if existing.is_empty() {
        let path = roots.first().map(|root| root.display().to_string());
        return Err(UsageError::NoLogs(path.unwrap_or_default()));
    }
    let mut files = Vec::new();
    for root in existing {
        collect_logs(root, &mut files).map_err(|error| UsageError::Io(error.to_string()))?;
    }
    // Only files that still exist keep a cursor.
    let mut next = Cursors::new();
    let mut found = Found::default();
    for file in files {
        let key = file.display().to_string();
        let cursor = cursors.remove(&key).unwrap_or_default();
        match read_file(&file, cursor, &mut found) {
            Ok(cursor) => {
                next.insert(key, cursor);
            }
            // A file can vanish between listing and reading, e.g. while
            // Codex compresses it; try again next time.
            Err(error) => tracing::debug!(%error, file = %file.display(), "log not readable"),
        }
    }
    Ok(UsageBatch {
        records: found.records,
        tool_calls: found.tool_calls,
        cursors: next,
        plan: found.plan,
    })
}

fn collect_logs(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            collect_logs(&path, files)?;
        } else if kind.is_file() && log_kind(&path).is_some() {
            files.push(path);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogKind {
    Plain,
    Compressed,
}

fn log_kind(path: &Path) -> Option<LogKind> {
    let has = |path: &Path, extension: &str| {
        path.extension()
            .is_some_and(|found| found.eq_ignore_ascii_case(extension))
    };
    if has(path, "jsonl") {
        Some(LogKind::Plain)
    } else if has(path, "zst") && has(Path::new(path.file_stem()?), "jsonl") {
        Some(LogKind::Compressed)
    } else {
        None
    }
}

#[derive(Default)]
struct Found {
    records: Vec<UsageRecord>,
    tool_calls: Vec<ToolCall>,
    plan: Option<PlanUsage>,
}

/// Reads the lines after `cursor`. Session and model are stated once
/// before the usage lines, so the whole file is parsed for them; only lines
/// after the cursor yield usage. An archive is written once and read whole.
fn read_file(path: &Path, cursor: FileCursor, found: &mut Found) -> std::io::Result<FileCursor> {
    let size = std::fs::metadata(path)?.len();
    let fallback = session_from_name(path);
    if log_kind(path) == Some(LogKind::Compressed) {
        if cursor.size == size && cursor.offset == size {
            return Ok(cursor);
        }
        let bytes = decompress(&std::fs::read(path)?)?;
        let mut cursor = FileCursor {
            size,
            offset: size,
            unreadable: 0,
        };
        parse(&bytes, 0, &fallback, &mut cursor, found);
        return Ok(cursor);
    }
    let mut cursor = if size < cursor.offset {
        FileCursor::default()
    } else {
        cursor
    };
    cursor.size = size;
    if size == cursor.offset {
        return Ok(cursor);
    }
    let bytes = std::fs::read(path)?;
    // A last line without newline is still being written; it is read next time.
    let Some(end) = bytes.iter().rposition(|&b| b == b'\n') else {
        return Ok(cursor);
    };
    let from = cursor.offset;
    parse(&bytes[..end], from, &fallback, &mut cursor, found);
    cursor.offset = u64::try_from(end + 1).unwrap_or(u64::MAX);
    cursor.size = u64::try_from(bytes.len()).unwrap_or(size);
    Ok(cursor)
}

/// Decodes every zstd frame of an archive.
fn decompress(mut compressed: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while !compressed.is_empty() {
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(&mut compressed)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        decoder.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

/// `rollout-2026-10-17T15-09-32-<id>.jsonl` → `<id>`, for logs without a session line.
fn session_from_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = name.split('.').next().unwrap_or_default();
    stem.rsplit_once('T')
        .and_then(|(_, rest)| rest.get(9..))
        .unwrap_or(stem)
        .to_owned()
}

/// What earlier lines of a file said about the lines after them.
struct Context {
    /// Identifies the conversation; part of every record key.
    thread: String,
    session: Option<String>,
    model: Option<ModelId>,
    cwd: Option<String>,
}

fn parse(bytes: &[u8], from: u64, fallback: &str, cursor: &mut FileCursor, found: &mut Found) {
    let mut context = Context {
        thread: fallback.to_owned(),
        session: None,
        model: None,
        cwd: None,
    };
    let mut position = 0u64;
    for line in bytes.split(|&b| b == b'\n') {
        let fresh = position >= from;
        position += u64::try_from(line.len()).unwrap_or(0) + 1;
        match parse_line(line, &mut context) {
            Parsed::Usage(record, plan) if fresh => {
                if let Some(record) = record {
                    found.records.push(*record);
                }
                if let Some(plan) = plan
                    && found.plan.as_ref().is_none_or(|known| known.at <= plan.at)
                {
                    found.plan = Some(plan);
                }
            }
            Parsed::ToolCall(call) if fresh => found.tool_calls.push(call),
            Parsed::Unreadable if fresh => cursor.unreadable += 1,
            Parsed::Usage(..) | Parsed::ToolCall(_) | Parsed::Unreadable | Parsed::Other => {}
        }
    }
}

enum Parsed {
    Usage(Option<Box<UsageRecord>>, Option<PlanUsage>),
    ToolCall(ToolCall),
    Unreadable,
    Other,
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<String>,
    #[serde(default)]
    payload: Payload,
}

/// The few payload fields hud reads; content fields are skipped unread.
#[derive(Deserialize, Default)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    call_id: Option<String>,
    info: Option<Info>,
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct Info {
    total_token_usage: Usage,
    last_token_usage: Usage,
}

#[derive(Deserialize)]
#[expect(clippy::struct_field_names, reason = "mirrors the JSON field names")]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

#[derive(Deserialize)]
struct RateLimits {
    plan_type: Option<String>,
    primary: Option<Window>,
    secondary: Option<Window>,
}

#[derive(Deserialize)]
#[expect(clippy::struct_field_names, reason = "mirrors the JSON field names")]
struct Window {
    used_percent: f64,
    window_minutes: Option<u32>,
    resets_at: Option<i64>,
    resets_in_seconds: Option<i64>,
}

fn parse_line(line: &[u8], context: &mut Context) -> Parsed {
    // Cheap pre-filter: most lines are messages, reasoning or tool output.
    let usage = contains(line, b"\"token_count\"");
    let relevant = usage
        || contains(line, b"\"session_meta\"")
        || contains(line, b"\"turn_context\"")
        || contains(line, b"\"call_id\"");
    if !relevant {
        return Parsed::Other;
    }
    let Ok(entry) = serde_json::from_slice::<Line>(line) else {
        return if usage {
            Parsed::Unreadable
        } else {
            Parsed::Other
        };
    };
    let at = entry.timestamp.and_then(|t| t.parse::<Timestamp>().ok());
    let payload = entry.payload;
    match (entry.kind.as_str(), payload.kind.as_deref()) {
        ("session_meta", _) => {
            if let Some(id) = payload.id {
                context.thread = id;
            }
            context.session = payload.session_id;
            context.cwd = payload.cwd.or(context.cwd.take());
            Parsed::Other
        }
        ("turn_context", _) => {
            context.model = payload
                .model
                .and_then(|model| ModelId::try_from(model).ok());
            context.cwd = payload.cwd.or(context.cwd.take());
            Parsed::Other
        }
        ("event_msg", Some("token_count")) => {
            let Some(at) = at else {
                return Parsed::Unreadable;
            };
            let plan = payload.rate_limits.and_then(|limits| plan(limits, at));
            let record = match payload.info {
                Some(info) => match record(info, at, context) {
                    Some(record) => Some(Box::new(record)),
                    None => return Parsed::Unreadable,
                },
                // Rate-limit updates come without token counts.
                None => None,
            };
            Parsed::Usage(record, plan)
        }
        // Calls end in `_call`; their results in `_call_output`.
        ("response_item", Some(kind)) if kind.ends_with("_call") => match (payload.call_id, at) {
            (Some(id), Some(at)) => Parsed::ToolCall(ToolCall {
                provider: Provider::Codex,
                id,
                at,
            }),
            _ => Parsed::Other,
        },
        _ => Parsed::Other,
    }
}

/// Codex reports each request twice or more with the same running total, so
/// the key is the conversation and its running total, not the line.
fn record(info: Info, at: Timestamp, context: &Context) -> Option<UsageRecord> {
    let model = context.model.clone()?;
    let last = info.last_token_usage;
    // Input includes the cached part; output includes reasoning.
    let cached = last.cached_input_tokens + last.cache_write_input_tokens;
    Some(UsageRecord {
        provider: Provider::Codex,
        key: format!("{}:{}", context.thread, info.total_token_usage.total_tokens),
        session: context
            .session
            .clone()
            .unwrap_or_else(|| context.thread.clone()),
        model,
        at,
        tokens: TokenUsage {
            input: last.input_tokens.saturating_sub(cached),
            output: last
                .output_tokens
                .saturating_sub(last.reasoning_output_tokens),
            cache_write_5m: last.cache_write_input_tokens,
            cache_write_1h: 0,
            cache_read: last.cached_input_tokens,
            reasoning: Some(last.reasoning_output_tokens),
        },
        speed: Speed::Standard,
        geo: Geo::Global,
        project: context.cwd.as_deref().and_then(|cwd| {
            Path::new(cwd)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        }),
        subagent: false,
        thinking: 0,
    })
}

fn plan(limits: RateLimits, at: Timestamp) -> Option<PlanUsage> {
    let window = |window: Option<Window>| {
        let window = window?;
        let resets_at = match (window.resets_at, window.resets_in_seconds) {
            (Some(seconds), _) => Timestamp::from_second(seconds).ok()?,
            (None, Some(seconds)) => at
                .checked_add(jiff::SignedDuration::from_secs(seconds))
                .ok()?,
            (None, None) => return None,
        };
        Some(PlanWindow {
            minutes: window.window_minutes,
            used_permille: permille(window.used_percent),
            resets_at,
        })
    };
    let primary = window(limits.primary);
    let secondary = window(limits.secondary);
    (primary.is_some() || secondary.is_some()).then_some(PlanUsage {
        at,
        plan: limits.plan_type,
        primary,
        secondary,
    })
}

/// A percentage such as `12.5` in per mille, within 0 to 1 000.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to a whole number between 0 and 1 000 first"
)]
fn permille(percent: f64) -> u16 {
    (percent * 10.0).round().clamp(0.0, 1000.0) as u16
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn fixtures() -> Vec<PathBuf> {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex");
        vec![base.join("sessions"), base.join("archived_sessions")]
    }

    fn by_key<'a>(batch: &'a UsageBatch, key: &str) -> &'a UsageRecord {
        batch
            .records
            .iter()
            .find(|r| r.key == key)
            .expect("BUG: record in fixture")
    }

    #[test]
    fn repeated_reports_of_one_request_share_a_key() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG: fixtures readable");
        let mut keys: Vec<&str> = batch.records.iter().map(|r| r.key.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "thread-0:580",
                "thread-1:1300",
                "thread-1:1300",
                "thread-1:3350"
            ],
            "the store merges the repeated report"
        );
        let unreadable: u64 = batch.cursors.values().map(|c| c.unreadable).sum();
        assert_eq!(unreadable, 1, "only the broken token count");
    }

    #[test]
    fn tokens_split_cached_input_and_reasoning() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG");
        let first = by_key(&batch, "thread-1:1300");
        assert_eq!(first.model.as_str(), "gpt-6-luna");
        assert_eq!(first.session, "session-1");
        assert_eq!(first.project.as_deref(), Some("hud"));
        assert_eq!(
            first.tokens,
            TokenUsage {
                input: 600,
                output: 200,
                cache_write_5m: 0,
                cache_write_1h: 0,
                cache_read: 400,
                reasoning: Some(100),
            }
        );
    }

    #[test]
    fn archives_are_decompressed() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG");
        let old = by_key(&batch, "thread-0:580");
        assert_eq!(old.model.as_str(), "gpt-5-codex");
        assert_eq!(old.project.as_deref(), Some("wiki"));
        let again = read_all(&fixtures(), batch.cursors).expect("BUG");
        assert!(
            again.records.is_empty(),
            "unchanged files are not read again"
        );
    }

    #[test]
    fn tool_calls_count_calls_and_not_their_results() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG");
        let mut ids: Vec<&str> = batch.tool_calls.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["call_0", "call_1"]);
    }

    #[test]
    fn the_newest_rate_limits_become_the_plan_usage() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG");
        let plan = batch.plan.expect("BUG: fixture has rate limits");
        assert_eq!(plan.at, "2026-10-17T13:10:16.988Z".parse().expect("BUG"));
        assert_eq!(plan.plan.as_deref(), Some("plus"));
        let primary = plan.primary.expect("BUG");
        assert_eq!(primary.used_permille, 140);
        assert_eq!(primary.minutes, Some(300));
        assert_eq!(plan.secondary.map(|w| w.used_permille), Some(30));
    }

    #[test]
    fn appended_lines_keep_the_model_named_before_the_cursor() {
        let dir = tempfile::tempdir().expect("BUG");
        let log = dir
            .path()
            .join("rollout-2026-10-17T15-09-32-thread-1.jsonl");
        std::fs::copy(
            fixtures()[0].join("2026/10/17/rollout-2026-10-17T15-09-32-thread-1.jsonl"),
            &log,
        )
        .expect("BUG");
        let roots = vec![dir.path().to_path_buf()];
        let first = read_all(&roots, Cursors::new()).expect("BUG");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .expect("BUG");
        writeln!(
            file,
            r#"{{"timestamp":"2026-10-17T13:20:00.000Z","ordinal":10,"type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"total_tokens":4000}},"last_token_usage":{{"input_tokens":600,"output_tokens":50,"total_tokens":650}}}}}}}}"#
        )
        .expect("BUG");
        let second = read_all(&roots, first.cursors).expect("BUG");
        assert_eq!(second.records.len(), 1);
        assert_eq!(second.records[0].key, "thread-1:4000");
        assert_eq!(second.records[0].model.as_str(), "gpt-6-luna");
        assert_eq!(
            second.cursors.values().map(|c| c.unreadable).sum::<u64>(),
            1,
            "the broken line before the cursor is not counted twice"
        );
    }

    #[test]
    fn missing_folders_mean_no_logs() {
        let error = read_all(&[PathBuf::from("/nonexistent/hud")], Cursors::new()).unwrap_err();
        assert!(matches!(error, UsageError::NoLogs(_)));
    }

    #[test]
    fn default_roots_respect_an_absolute_codex_home() {
        let home = Path::new("/Users/test");
        assert_eq!(
            CodexLogs::default_roots(home, None),
            vec![
                PathBuf::from("/Users/test/.codex/sessions"),
                PathBuf::from("/Users/test/.codex/archived_sessions")
            ]
        );
        assert_eq!(
            CodexLogs::default_roots(home, Some(Path::new("/cx")))[0],
            Path::new("/cx/sessions")
        );
    }

    #[test]
    fn percentages_become_per_mille_within_bounds() {
        assert_eq!(permille(12.5), 125);
        assert_eq!(permille(-1.0), 0);
        assert_eq!(permille(250.0), 1000);
    }
}
