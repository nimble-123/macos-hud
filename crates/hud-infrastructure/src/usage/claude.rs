//! Claude Code usage from `~/.claude/projects/**/*.jsonl`.
//!
//! Only usage metadata is parsed; prompt and answer content is never kept.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use futures::future::BoxFuture;
use hud_application::ports::usage::{Cursors, FileCursor, UsageBatch, UsageError, UsageSource};
use hud_domain::usage::{Geo, ModelId, Provider, Speed, TokenUsage, ToolCall, UsageRecord};
use serde::Deserialize;

/// Reads Claude Code's session logs, from the CLI and the desktop app alike.
#[derive(Debug, Clone)]
pub struct ClaudeLogs {
    root: PathBuf,
}

impl ClaudeLogs {
    /// Logs below `root`, usually `~/.claude/projects`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `$CLAUDE_CONFIG_DIR/projects`, else `~/.claude/projects`.
    #[must_use]
    pub fn default_root(home: &Path, config_dir: Option<&Path>) -> PathBuf {
        config_dir
            .filter(|dir| dir.is_absolute())
            .map_or_else(|| home.join(".claude"), Path::to_path_buf)
            .join("projects")
    }
}

impl UsageSource for ClaudeLogs {
    fn provider(&self) -> Provider {
        Provider::Claude
    }

    fn read(&self, cursors: Cursors) -> BoxFuture<'_, Result<UsageBatch, UsageError>> {
        let root = self.root.clone();
        Box::pin(async move {
            // Plain file I/O and parsing; the first read can be hundreds of MB.
            tokio::task::spawn_blocking(move || read_all(&root, cursors))
                .await
                .map_err(|error| UsageError::Io(error.to_string()))?
        })
    }
}

fn read_all(root: &Path, mut cursors: Cursors) -> Result<UsageBatch, UsageError> {
    if !root.is_dir() {
        return Err(UsageError::NoLogs(root.display().to_string()));
    }
    let mut files = Vec::new();
    collect_logs(root, &mut files).map_err(|error| UsageError::Io(error.to_string()))?;
    let mut found = Found::default();
    for file in files {
        let key = file.display().to_string();
        let cursor = cursors.get(&key).copied().unwrap_or_default();
        let context = FileContext::new(root, &file);
        match read_file(&file, cursor, &context, &mut found) {
            Ok(cursor) => {
                cursors.insert(key, cursor);
            }
            // A file can vanish between listing and reading; try again next time.
            Err(error) => tracing::debug!(%error, file = %file.display(), "log not readable"),
        }
    }
    Ok(UsageBatch {
        records: found.records,
        tool_calls: found.tool_calls,
        cursors,
        plan: None,
    })
}

fn collect_logs(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            collect_logs(&path, files)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
            files.push(path);
        }
    }
    Ok(())
}

#[derive(Default)]
struct Found {
    records: Vec<UsageRecord>,
    tool_calls: Vec<ToolCall>,
}

/// What the location of a log file says about its lines.
struct FileContext {
    /// Session id if a line has none: the file name.
    session: String,
    /// Folder name below the root; Claude Code derives it from the start directory.
    project_dir: String,
    /// Logs below `subagents/` belong to subagents.
    subagent: bool,
}

impl FileContext {
    fn new(root: &Path, file: &Path) -> Self {
        let relative = file.strip_prefix(root).unwrap_or(file);
        Self {
            session: file
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            project_dir: relative
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .unwrap_or_default(),
            subagent: relative.components().any(|c| c.as_os_str() == "subagents"),
        }
    }

    /// The project root is the ancestor of `cwd` whose name Claude Code
    /// turned into the folder name; a session in a sub-folder still counts
    /// for its project. Without a match the folder of `cwd` is used.
    fn project(&self, cwd: &str) -> Option<String> {
        let cwd = Path::new(cwd);
        let root = cwd
            .ancestors()
            .find(|dir| sanitize(dir) == self.project_dir)
            .unwrap_or(cwd);
        root.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

/// Claude Code's folder names: every character except ASCII letters and digits becomes `-`.
fn sanitize(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Reads complete lines after `cursor`; a file shorter than the cursor was
/// rewritten and is read from the start.
fn read_file(
    path: &Path,
    cursor: FileCursor,
    context: &FileContext,
    found: &mut Found,
) -> std::io::Result<FileCursor> {
    let size = std::fs::metadata(path)?.len();
    let mut cursor = if size < cursor.offset {
        FileCursor::default()
    } else {
        cursor
    };
    cursor.size = size;
    if size == cursor.offset {
        return Ok(cursor);
    }
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(cursor.offset))?;
    let mut bytes = Vec::new();
    file.take(size - cursor.offset).read_to_end(&mut bytes)?;
    // A last line without newline is still being written; it is read next time.
    let Some(end) = bytes.iter().rposition(|&b| b == b'\n') else {
        return Ok(cursor);
    };
    for line in bytes[..end].split(|&b| b == b'\n') {
        match parse_line(line, context) {
            Parsed::Record(record, calls) => {
                found.tool_calls.extend(calls);
                found.records.push(*record);
            }
            Parsed::Unreadable => cursor.unreadable += 1,
            Parsed::Other => {}
        }
    }
    cursor.offset += u64::try_from(end + 1).unwrap_or(0);
    Ok(cursor)
}

enum Parsed {
    Record(Box<UsageRecord>, Vec<ToolCall>),
    Unreadable,
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<String>,
    session_id: Option<String>,
    request_id: Option<String>,
    cwd: Option<String>,
    #[serde(default)]
    is_sidechain: bool,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
    /// Only the block type and the id of tool calls are read.
    #[serde(default)]
    content: Vec<Block>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_creation: Option<CacheCreation>,
    speed: Option<String>,
    inference_geo: Option<String>,
    output_tokens_details: Option<OutputDetails>,
}

#[derive(Deserialize)]
struct OutputDetails {
    thinking_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

fn parse_line(line: &[u8], context: &FileContext) -> Parsed {
    // Cheap pre-filter: most lines are prompts, tool output or metadata.
    let looks_like_usage = memchr_str(line, b"\"assistant\"") && memchr_str(line, b"\"usage\"");
    if !looks_like_usage {
        return Parsed::Other;
    }
    let Ok(entry) = serde_json::from_slice::<Entry>(line) else {
        return Parsed::Unreadable;
    };
    if entry.kind != "assistant" {
        return Parsed::Other;
    }
    let Some(message) = entry.message else {
        return Parsed::Other;
    };
    let Some(usage) = message.usage else {
        return Parsed::Other;
    };
    // Claude Code writes local placeholder answers as model `<synthetic>`.
    let model = match message.model {
        Some(model) if model.starts_with('<') => return Parsed::Other,
        Some(model) => model,
        None => return Parsed::Unreadable,
    };
    let (Some(id), Some(Ok(at)), Ok(model)) = (
        message.id,
        entry.timestamp.map(|t| t.parse()),
        ModelId::try_from(model),
    ) else {
        return Parsed::Unreadable;
    };
    let written = usage.cache_creation_input_tokens.unwrap_or(0);
    // Older logs have no split by cache lifetime; they used the 5-minute cache.
    let (short_lived, long_lived) = match usage.cache_creation {
        Some(split) => (
            split.ephemeral_5m_input_tokens.unwrap_or(0),
            split.ephemeral_1h_input_tokens.unwrap_or(0),
        ),
        None => (written, 0),
    };
    let key = match entry.request_id {
        Some(request) => format!("{id}:{request}"),
        None => id,
    };
    let calls = message
        .content
        .into_iter()
        .filter(|block| block.kind.as_deref() == Some("tool_use"))
        .filter_map(|block| block.id)
        .map(|id| ToolCall {
            provider: Provider::Claude,
            id,
            at,
        })
        .collect();
    let record = UsageRecord {
        provider: Provider::Claude,
        key,
        session: entry.session_id.unwrap_or_else(|| context.session.clone()),
        model,
        at,
        tokens: TokenUsage {
            input: usage.input_tokens.unwrap_or(0),
            output: usage.output_tokens.unwrap_or(0),
            cache_write_5m: short_lived,
            cache_write_1h: long_lived,
            cache_read: usage.cache_read_input_tokens.unwrap_or(0),
            reasoning: None,
        },
        speed: match usage.speed.as_deref() {
            Some("fast") => Speed::Fast,
            _ => Speed::Standard,
        },
        geo: match usage.inference_geo.as_deref() {
            Some("us") => Geo::Us,
            _ => Geo::Global,
        },
        project: entry.cwd.as_deref().and_then(|cwd| context.project(cwd)),
        subagent: entry.is_sidechain || context.subagent,
        thinking: usage
            .output_tokens_details
            .and_then(|details| details.thinking_tokens)
            .unwrap_or(0),
    };
    Parsed::Record(Box::new(record), calls)
}

fn memchr_str(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude")
    }

    #[test]
    fn reads_usage_from_nested_logs_and_skips_everything_else() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG: fixtures readable");
        let keys: Vec<&str> = batch.records.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys.iter().filter(|k| k.starts_with("msg_a1")).count(),
            2,
            "two lines of the same answer, merged later by the store: {keys:?}"
        );
        assert!(
            keys.contains(&"msg_sub1:req_sub1"),
            "subagent logs are read: {keys:?}"
        );
        assert!(!keys.iter().any(|k| k.contains("synthetic")));
        let unreadable: u64 = batch.cursors.values().map(|c| c.unreadable).sum();
        assert_eq!(unreadable, 1);
    }

    #[test]
    fn cache_writes_are_split_by_lifetime() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG");
        let first = batch
            .records
            .iter()
            .find(|r| r.key == "msg_b1:req_b1")
            .expect("BUG");
        assert_eq!(first.tokens.cache_write_1h, 27_490);
        assert_eq!(first.tokens.cache_write_5m, 0);
        assert_eq!(first.speed, Speed::Fast);
        let old = batch
            .records
            .iter()
            .find(|r| r.key == "msg_old:req_old")
            .expect("BUG");
        assert_eq!(
            old.tokens.cache_write_5m, 1_200,
            "no split means 5-minute cache"
        );
    }

    #[test]
    fn a_second_read_only_sees_appended_complete_lines() {
        let dir = tempfile::tempdir().expect("BUG");
        let log = dir.path().join("s.jsonl");
        let line = std::fs::read_to_string(fixtures().join("project-a/session-1.jsonl"))
            .expect("BUG")
            .lines()
            .find(|l| l.contains("msg_b1"))
            .expect("BUG")
            .to_owned();
        std::fs::write(&log, format!("{line}\n")).expect("BUG");
        let first = read_all(dir.path(), Cursors::new()).expect("BUG");
        assert_eq!(first.records.len(), 1);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .expect("BUG");
        write!(file, "{}", line.replace("msg_b1", "msg_b2")).expect("BUG");
        let partial = read_all(dir.path(), first.cursors).expect("BUG");
        assert!(partial.records.is_empty(), "the unfinished line waits");

        writeln!(file).expect("BUG");
        let second = read_all(dir.path(), partial.cursors).expect("BUG");
        assert_eq!(second.records.len(), 1);
        assert!(second.records[0].key.starts_with("msg_b2"));
    }

    #[test]
    fn a_rewritten_shorter_file_is_read_from_the_start() {
        let dir = tempfile::tempdir().expect("BUG");
        let log = dir.path().join("s.jsonl");
        let text =
            std::fs::read_to_string(fixtures().join("project-a/session-1.jsonl")).expect("BUG");
        std::fs::write(&log, &text).expect("BUG");
        let first = read_all(dir.path(), Cursors::new()).expect("BUG");
        let line = text.lines().find(|l| l.contains("msg_b1")).expect("BUG");
        std::fs::write(&log, format!("{line}\n")).expect("BUG");
        let again = read_all(dir.path(), first.cursors).expect("BUG");
        assert_eq!(again.records.len(), 1);
    }

    #[test]
    fn subagents_projects_thinking_and_tool_calls_are_read() {
        let batch = read_all(&fixtures(), Cursors::new()).expect("BUG");
        let sub = batch
            .records
            .iter()
            .find(|r| r.key == "msg_sub1:req_sub1")
            .expect("BUG");
        assert!(sub.subagent, "file below subagents/");
        let main = batch
            .records
            .iter()
            .find(|r| r.key == "msg_b1:req_b1")
            .expect("BUG");
        assert!(!main.subagent);
        assert_eq!(main.thinking, 36);
        assert_eq!(
            main.project.as_deref(),
            Some("src"),
            "no ancestor matches the fixture folder"
        );
        let ids: Vec<&str> = batch.tool_calls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["toolu_1"]);
    }

    #[test]
    fn a_session_in_a_sub_folder_counts_for_its_project() {
        let context = FileContext {
            session: String::new(),
            project_dir: "-Volumes-1TB-EXT-workspace-macos-hud".into(),
            subagent: false,
        };
        assert_eq!(
            context
                .project("/Volumes/1TB EXT/workspace/macos-hud/crates/hud-tui")
                .as_deref(),
            Some("macos-hud")
        );
        assert_eq!(context.project("/elsewhere/tool").as_deref(), Some("tool"));
    }

    #[test]
    fn a_missing_folder_means_no_logs() {
        let error = read_all(Path::new("/nonexistent/hud"), Cursors::new()).unwrap_err();
        assert!(matches!(error, UsageError::NoLogs(_)));
    }

    #[test]
    fn default_root_respects_an_absolute_config_dir() {
        let home = Path::new("/Users/test");
        assert_eq!(
            ClaudeLogs::default_root(home, None),
            Path::new("/Users/test/.claude/projects")
        );
        assert_eq!(
            ClaudeLogs::default_root(home, Some(Path::new("/cfg"))),
            Path::new("/cfg/projects")
        );
    }
}
