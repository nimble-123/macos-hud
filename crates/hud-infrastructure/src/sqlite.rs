//! SQLite persistence behind a dedicated actor thread (ADR 0006).

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use hud_application::ports::store::{
    ActionLog, CachedRepo, InstallHistory, InstallResult, ScanCache, StoreError,
};
use hud_application::ports::telemetry::TelemetryStore;
use hud_application::ports::usage::{Cursors, FileCursor, UsageBatch, UsageStore};
use hud_domain::action::{ActionId, ActionKind, ActionRecord, Outcome, Trigger};
use hud_domain::git::{BranchName, CommitId, GitOperation, Head, RepoPath, RepoStatus, Upstream};
use hud_domain::update::{PackageName, SourceId, UpdateCandidate};
use hud_domain::usage::{
    Geo, ModelId, PlanUsage, PlanWindow, Provider, Speed, TelemetryMetric, TelemetryPoint,
    TelemetryTotals, TokenUsage, UsageRecord,
};
use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};
use tokio::sync::{mpsc, oneshot};

/// Queue length towards the DB thread; callers wait when it is full.
const QUEUE: usize = 64;
/// Log entries older than this are deleted at start-up.
const ACTION_LOG_RETENTION: Duration = Duration::from_hours(2160);

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            "CREATE TABLE action_log (
            id INTEGER PRIMARY KEY,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            kind TEXT NOT NULL,
            target TEXT NOT NULL,
            trigger TEXT NOT NULL,
            outcome TEXT,
            outcome_detail TEXT,
            summary TEXT
        );
        CREATE INDEX action_log_started_at ON action_log (started_at);
        CREATE TABLE repo_cache (
            path TEXT PRIMARY KEY,
            head TEXT,
            head_oid TEXT,
            upstream TEXT,
            upstream_oid TEXT,
            ahead INTEGER NOT NULL,
            behind INTEGER NOT NULL,
            tracked_changes INTEGER NOT NULL,
            untracked INTEGER NOT NULL,
            operation TEXT,
            scanned_at TEXT NOT NULL
        );",
        ),
        M::up(
            "CREATE TABLE install_history (
                id INTEGER PRIMARY KEY,
                at TEXT NOT NULL,
                source TEXT NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                result TEXT NOT NULL
            );
            CREATE INDEX install_history_package ON install_history (source, name);",
        ),
        M::up(
            "CREATE TABLE ai_usage (
                provider TEXT NOT NULL,
                message_key TEXT NOT NULL,
                session TEXT NOT NULL,
                model TEXT NOT NULL,
                at_ms INTEGER NOT NULL,
                input INTEGER NOT NULL,
                output INTEGER NOT NULL,
                cache_write_5m INTEGER NOT NULL,
                cache_write_1h INTEGER NOT NULL,
                cache_read INTEGER NOT NULL,
                reasoning INTEGER,
                speed TEXT NOT NULL,
                geo TEXT NOT NULL,
                PRIMARY KEY (provider, message_key)
            );
            CREATE INDEX ai_usage_at ON ai_usage (at_ms);
            CREATE TABLE ingest_offset (
                provider TEXT NOT NULL,
                file TEXT NOT NULL,
                size INTEGER NOT NULL,
                byte_offset INTEGER NOT NULL,
                unreadable INTEGER NOT NULL,
                PRIMARY KEY (provider, file)
            );",
        ),
        M::up(
            "CREATE TABLE otel_metric (
                id INTEGER PRIMARY KEY,
                at_ms INTEGER NOT NULL,
                metric TEXT NOT NULL,
                detail TEXT,
                model TEXT,
                session TEXT,
                value INTEGER NOT NULL
            );
            CREATE INDEX otel_metric_at ON otel_metric (at_ms);",
        ),
        // The new columns are filled from the logs: read everything once more.
        M::up(
            "ALTER TABLE ai_usage ADD COLUMN project TEXT;
            ALTER TABLE ai_usage ADD COLUMN subagent INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE ai_usage ADD COLUMN thinking INTEGER NOT NULL DEFAULT 0;
            CREATE TABLE ai_tool_call (
                provider TEXT NOT NULL,
                call_id TEXT NOT NULL,
                at_ms INTEGER NOT NULL,
                PRIMARY KEY (provider, call_id)
            );
            CREATE INDEX ai_tool_call_at ON ai_tool_call (at_ms);
            DELETE FROM ingest_offset;",
        ),
        M::up(PLAN_USAGE),
    ])
}

/// Codex reports its context window and subscription usage.
const PLAN_USAGE: &str = "ALTER TABLE ai_usage ADD COLUMN context_window INTEGER;
    CREATE TABLE ai_plan_usage (
        provider TEXT PRIMARY KEY,
        at_ms INTEGER NOT NULL,
        plan TEXT,
        primary_permille INTEGER,
        primary_minutes INTEGER,
        primary_resets_ms INTEGER,
        secondary_permille INTEGER,
        secondary_minutes INTEGER,
        secondary_resets_ms INTEGER
    );";

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// Handle to the DB thread. Cheap to clone; the thread ends when the last handle drops.
#[derive(Debug, Clone)]
pub struct SqliteStore {
    jobs: mpsc::Sender<Job>,
}

impl SqliteStore {
    /// Opens (or creates) the database, migrates it and prunes old log entries.
    pub fn open(path: &Path, now: Timestamp) -> Result<Self, StoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(error)?;
        }
        Self::start(Connection::open(path).map_err(error)?, now)
    }

    /// An in-memory database for tests.
    pub fn in_memory(now: Timestamp) -> Result<Self, StoreError> {
        Self::start(Connection::open_in_memory().map_err(error)?, now)
    }

    fn start(mut connection: Connection, now: Timestamp) -> Result<Self, StoreError> {
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .and_then(|()| connection.pragma_update(None, "foreign_keys", "ON"))
            .map_err(error)?;
        migrations().to_latest(&mut connection).map_err(error)?;
        let cutoff = now.checked_sub(ACTION_LOG_RETENTION).unwrap_or(now);
        connection
            .execute(
                "DELETE FROM action_log WHERE started_at < ?1",
                params![cutoff.to_string()],
            )
            .map_err(error)?;

        let (jobs, mut queue) = mpsc::channel::<Job>(QUEUE);
        std::thread::Builder::new()
            .name("hud-sqlite".into())
            .spawn(move || {
                while let Some(job) = queue.blocking_recv() {
                    job(&mut connection);
                }
            })
            .map_err(error)?;
        Ok(Self { jobs })
    }

    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T, StoreError> {
        let (reply, answer) = oneshot::channel();
        let job: Job = Box::new(move |connection| {
            // The caller may have gone away; then nobody needs the answer.
            let _ = reply.send(work(connection));
        });
        self.jobs.send(job).await.map_err(|_| stopped())?;
        answer.await.map_err(|_| stopped())?.map_err(error)
    }
}

fn error(source: impl std::fmt::Display) -> StoreError {
    StoreError {
        message: source.to_string(),
    }
}

fn stopped() -> StoreError {
    StoreError {
        message: "database thread stopped".into(),
    }
}

impl ActionLog for SqliteStore {
    async fn start(
        &self,
        at: Timestamp,
        kind: ActionKind,
        target: &str,
        trigger: Trigger,
    ) -> Result<ActionId, StoreError> {
        let target = target.to_owned();
        self.call(move |db| {
            db.execute(
                "INSERT INTO action_log (started_at, kind, target, trigger) VALUES (?1, ?2, ?3, ?4)",
                params![at.to_string(), kind_str(kind), target, trigger_str(trigger)],
            )?;
            Ok(ActionId(db.last_insert_rowid()))
        })
        .await
    }

    async fn finish(
        &self,
        id: ActionId,
        at: Timestamp,
        outcome: Outcome,
        summary: Option<String>,
    ) -> Result<(), StoreError> {
        self.call(move |db| {
            let (name, detail) = outcome_columns(&outcome);
            db.execute(
                "UPDATE action_log SET finished_at = ?1, outcome = ?2, outcome_detail = ?3, summary = ?4 WHERE id = ?5",
                params![at.to_string(), name, detail, summary, id.0],
            )?;
            Ok(())
        })
        .await
    }

    async fn recent(&self, limit: u32) -> Result<Vec<ActionRecord>, StoreError> {
        self.call(move |db| {
            let mut query = db.prepare(
                "SELECT id, started_at, finished_at, kind, target, trigger, outcome, outcome_detail, summary
                 FROM action_log ORDER BY started_at DESC, id DESC LIMIT ?1",
            )?;
            let rows = query.query_map(params![limit], |row| {
                let started: String = row.get(1)?;
                let finished: Option<String> = row.get(2)?;
                let outcome: Option<String> = row.get(6)?;
                let detail: Option<String> = row.get(7)?;
                Ok(ActionRecord {
                    id: ActionId(row.get(0)?),
                    started_at: parse_ts(&started, 1)?,
                    finished_at: finished.as_deref().map(|ts| parse_ts(ts, 2)).transpose()?,
                    kind: parse_kind(&row.get::<_, String>(3)?, 3)?,
                    target: row.get(4)?,
                    trigger: parse_trigger(&row.get::<_, String>(5)?, 5)?,
                    outcome: outcome.as_deref().map(|name| parse_outcome(name, detail, 6)).transpose()?,
                    summary: row.get(8)?,
                })
            })?;
            rows.collect()
        })
        .await
    }
}

impl ScanCache for SqliteStore {
    async fn save(&self, entry: CachedRepo) -> Result<(), StoreError> {
        self.call(move |db| {
            let status = &entry.status;
            let head = match &status.head {
                Head::Branch(name) => Some(name.as_str().to_owned()),
                Head::Detached => None,
            };
            db.execute(
                "INSERT OR REPLACE INTO repo_cache
                 (path, head, head_oid, upstream, upstream_oid, ahead, behind, tracked_changes, untracked, operation, scanned_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    entry.repo.to_string(),
                    head,
                    status.head_oid.as_ref().map(CommitId::as_str),
                    status.upstream.as_ref().map(|u| u.name.as_str()),
                    status.upstream.as_ref().map(|u| u.oid.as_str()),
                    status.ahead,
                    status.behind,
                    status.tracked_changes,
                    status.untracked,
                    status.operation.map(operation_str),
                    entry.scanned_at.to_string(),
                ],
            )?;
            Ok(())
        })
        .await
    }

    async fn load_all(&self) -> Result<Vec<CachedRepo>, StoreError> {
        self.call(|db| {
            let mut query = db.prepare(
                "SELECT path, head, head_oid, upstream, upstream_oid, ahead, behind, tracked_changes, untracked, operation, scanned_at
                 FROM repo_cache ORDER BY path",
            )?;
            let rows = query.query_map([], |row| {
                let path: String = row.get(0)?;
                let head: Option<String> = row.get(1)?;
                let head_oid: Option<String> = row.get(2)?;
                let upstream: Option<String> = row.get(3)?;
                let upstream_oid: Option<String> = row.get(4)?;
                let operation: Option<String> = row.get(9)?;
                let scanned: String = row.get(10)?;
                let upstream = match (upstream, upstream_oid) {
                    (Some(name), Some(oid)) => Some(Upstream { name, oid: parse_commit(&oid, 4)? }),
                    _ => None,
                };
                Ok(CachedRepo {
                    repo: RepoPath::try_from(std::path::PathBuf::from(path)).map_err(|e| conversion(0, e))?,
                    status: RepoStatus {
                        head: match head {
                            Some(name) => Head::Branch(BranchName::try_from(name).map_err(|e| conversion(1, e))?),
                            None => Head::Detached,
                        },
                        head_oid: head_oid.as_deref().map(|oid| parse_commit(oid, 2)).transpose()?,
                        upstream,
                        ahead: row.get(5)?,
                        behind: row.get(6)?,
                        tracked_changes: row.get(7)?,
                        untracked: row.get(8)?,
                        operation: operation.as_deref().map(|op| parse_operation(op, 9)).transpose()?,
                    },
                    scanned_at: parse_ts(&scanned, 10)?,
                })
            })?;
            rows.collect()
        })
        .await
    }

    async fn retain(&self, repos: Vec<RepoPath>) -> Result<(), StoreError> {
        self.call(move |db| {
            let keep: std::collections::HashSet<String> =
                repos.iter().map(ToString::to_string).collect();
            let transaction = db.transaction()?;
            let cached: Vec<String> = transaction
                .prepare("SELECT path FROM repo_cache")?
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for path in cached.iter().filter(|path| !keep.contains(*path)) {
                transaction.execute("DELETE FROM repo_cache WHERE path = ?1", params![path])?;
            }
            transaction.commit()
        })
        .await
    }
}

impl InstallHistory for SqliteStore {
    async fn record(
        &self,
        at: Timestamp,
        candidate: &UpdateCandidate,
        result: InstallResult,
    ) -> Result<(), StoreError> {
        let source = source_str(candidate.source);
        let name = candidate.name.to_string();
        let version = candidate.latest.to_string();
        self.call(move |db| {
            db.execute(
                "INSERT INTO install_history (at, source, name, version, result) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![at.to_string(), source, name, version, result_str(result)],
            )?;
            Ok(())
        })
        .await
    }

    async fn needed_interaction(
        &self,
        source: SourceId,
        name: &PackageName,
    ) -> Result<bool, StoreError> {
        let source = source_str(source);
        let name = name.to_string();
        self.call(move |db| {
            let latest: Option<String> = db
                .query_row(
                    "SELECT result FROM install_history WHERE source = ?1 AND name = ?2 ORDER BY id DESC LIMIT 1",
                    params![source, name],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(latest.as_deref() == Some(result_str(InstallResult::NeedsInteraction)))
        })
        .await
    }
}

impl UsageStore for SqliteStore {
    async fn cursors(&self, provider: Provider) -> Result<Cursors, StoreError> {
        self.call(move |db| {
            let mut query = db.prepare(
                "SELECT file, size, byte_offset, unreadable FROM ingest_offset WHERE provider = ?1",
            )?;
            let rows = query.query_map(params![provider_str(provider)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    FileCursor {
                        size: get_u64(row, 1)?,
                        offset: get_u64(row, 2)?,
                        unreadable: get_u64(row, 3)?,
                    },
                ))
            })?;
            rows.collect()
        })
        .await
    }

    async fn save(&self, provider: Provider, batch: UsageBatch) -> Result<(), StoreError> {
        self.call(move |db| {
            let name = provider_str(provider);
            let transaction = db.transaction()?;
            {
                // Lines of the same answer repeat with growing counts: keep the highest.
                let mut insert = transaction.prepare(
                    "INSERT INTO ai_usage (provider, message_key, session, model, at_ms, input, output,
                         cache_write_5m, cache_write_1h, cache_read, reasoning, speed, geo,
                         project, subagent, thinking, context_window)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                     ON CONFLICT (provider, message_key) DO UPDATE SET
                         project = coalesce(excluded.project, project),
                         context_window = coalesce(excluded.context_window, context_window),
                         subagent = max(subagent, excluded.subagent),
                         thinking = max(thinking, excluded.thinking),
                         input = max(input, excluded.input),
                         output = max(output, excluded.output),
                         cache_write_5m = max(cache_write_5m, excluded.cache_write_5m),
                         cache_write_1h = max(cache_write_1h, excluded.cache_write_1h),
                         cache_read = max(cache_read, excluded.cache_read),
                         reasoning = max(coalesce(reasoning, 0), coalesce(excluded.reasoning, 0))",
                )?;
                for record in &batch.records {
                    let tokens = &record.tokens;
                    insert.execute(params![
                        name,
                        record.key,
                        record.session,
                        record.model.as_str(),
                        record.at.as_millisecond(),
                        int(tokens.input),
                        int(tokens.output),
                        int(tokens.cache_write_5m),
                        int(tokens.cache_write_1h),
                        int(tokens.cache_read),
                        tokens.reasoning.map(int),
                        speed_str(record.speed),
                        geo_str(record.geo),
                        record.project,
                        record.subagent,
                        int(record.thinking),
                        record.context_window.map(int),
                    ])?;
                }
                let mut call = transaction.prepare(
                    "INSERT OR IGNORE INTO ai_tool_call (provider, call_id, at_ms) VALUES (?1, ?2, ?3)",
                )?;
                for tool_call in &batch.tool_calls {
                    call.execute(params![name, tool_call.id, tool_call.at.as_millisecond()])?;
                }
                transaction.execute("DELETE FROM ingest_offset WHERE provider = ?1", params![name])?;
                let mut cursor = transaction.prepare(
                    "INSERT INTO ingest_offset (provider, file, size, byte_offset, unreadable)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )?;
                for (file, position) in &batch.cursors {
                    cursor.execute(params![
                        name,
                        file,
                        int(position.size),
                        int(position.offset),
                        int(position.unreadable)
                    ])?;
                }
                if let Some(plan) = &batch.plan {
                    save_plan(&transaction, name, plan)?;
                }
            }
            transaction.commit()
        })
        .await
    }

    async fn records(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Vec<UsageRecord>, StoreError> {
        self.call(move |db| {
            let mut query = db.prepare(
                "SELECT provider, message_key, session, model, at_ms, input, output, cache_write_5m,
                        cache_write_1h, cache_read, reasoning, speed, geo, project, subagent, thinking,
                        context_window
                 FROM ai_usage WHERE at_ms >= ?1 AND at_ms < ?2",
            )?;
            let rows =
                query.query_map(params![from.as_millisecond(), to.as_millisecond()], |row| {
                    Ok(UsageRecord {
                        provider: parse_provider(&row.get::<_, String>(0)?, 0)?,
                        key: row.get(1)?,
                        session: row.get(2)?,
                        model: ModelId::try_from(row.get::<_, String>(3)?)
                            .map_err(|e| conversion(3, e))?,
                        at: Timestamp::from_millisecond(row.get(4)?)
                            .map_err(|e| conversion(4, e))?,
                        tokens: TokenUsage {
                            input: get_u64(row, 5)?,
                            output: get_u64(row, 6)?,
                            cache_write_5m: get_u64(row, 7)?,
                            cache_write_1h: get_u64(row, 8)?,
                            cache_read: get_u64(row, 9)?,
                            reasoning: row
                                .get::<_, Option<i64>>(10)?
                                .map(|v| u64::try_from(v).map_err(|e| conversion(10, e)))
                                .transpose()?,
                        },
                        speed: if row.get::<_, String>(11)? == "fast" {
                            Speed::Fast
                        } else {
                            Speed::Standard
                        },
                        geo: if row.get::<_, String>(12)? == "us" {
                            Geo::Us
                        } else {
                            Geo::Global
                        },
                        project: row.get(13)?,
                        subagent: row.get(14)?,
                        thinking: get_u64(row, 15)?,
                        context_window: row
                            .get::<_, Option<i64>>(16)?
                            .map(|v| u64::try_from(v).map_err(|e| conversion(16, e)))
                            .transpose()?,
                    })
                })?;
            rows.collect()
        })
        .await
    }

    async fn answer_times(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Vec<Timestamp>, StoreError> {
        self.call(move |db| {
            let mut query =
                db.prepare("SELECT at_ms FROM ai_usage WHERE at_ms >= ?1 AND at_ms < ?2")?;
            let rows = query
                .query_map(params![from.as_millisecond(), to.as_millisecond()], |row| {
                    Timestamp::from_millisecond(row.get(0)?).map_err(|e| conversion(0, e))
                })?;
            rows.collect()
        })
        .await
    }

    async fn tool_calls(
        &self,
        provider: Provider,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<u64, StoreError> {
        self.call(move |db| {
            db.query_row(
                "SELECT count(*) FROM ai_tool_call
                 WHERE provider = ?1 AND at_ms >= ?2 AND at_ms < ?3",
                params![
                    provider_str(provider),
                    from.as_millisecond(),
                    to.as_millisecond()
                ],
                |row| get_u64(row, 0),
            )
        })
        .await
    }

    async fn plan(&self, provider: Provider) -> Result<Option<PlanUsage>, StoreError> {
        self.call(move |db| {
            db.query_row(
                "SELECT at_ms, plan, primary_permille, primary_minutes, primary_resets_ms,
                        secondary_permille, secondary_minutes, secondary_resets_ms
                 FROM ai_plan_usage WHERE provider = ?1",
                params![provider_str(provider)],
                |row| {
                    Ok(PlanUsage {
                        at: Timestamp::from_millisecond(row.get(0)?)
                            .map_err(|e| conversion(0, e))?,
                        plan: row.get(1)?,
                        primary: plan_window(row, 2)?,
                        secondary: plan_window(row, 5)?,
                    })
                },
            )
            .optional()
        })
        .await
    }
}

/// Keeps the plan usage only if it is at least as new as the stored one,
/// because files are not read in the order they were written.
fn save_plan(
    transaction: &rusqlite::Transaction<'_>,
    provider: &str,
    plan: &PlanUsage,
) -> rusqlite::Result<()> {
    let window = |window: Option<PlanWindow>| {
        (
            window.map(|w| w.used_permille),
            window.and_then(|w| w.minutes),
            window.map(|w| w.resets_at.as_millisecond()),
        )
    };
    let (primary_permille, primary_minutes, primary_resets) = window(plan.primary);
    let (secondary_permille, secondary_minutes, secondary_resets) = window(plan.secondary);
    transaction.execute(
        "INSERT INTO ai_plan_usage (provider, at_ms, plan, primary_permille, primary_minutes,
             primary_resets_ms, secondary_permille, secondary_minutes, secondary_resets_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (provider) DO UPDATE SET
             at_ms = excluded.at_ms,
             plan = excluded.plan,
             primary_permille = excluded.primary_permille,
             primary_minutes = excluded.primary_minutes,
             primary_resets_ms = excluded.primary_resets_ms,
             secondary_permille = excluded.secondary_permille,
             secondary_minutes = excluded.secondary_minutes,
             secondary_resets_ms = excluded.secondary_resets_ms
         WHERE excluded.at_ms >= ai_plan_usage.at_ms",
        params![
            provider,
            plan.at.as_millisecond(),
            plan.plan,
            primary_permille,
            primary_minutes,
            primary_resets,
            secondary_permille,
            secondary_minutes,
            secondary_resets
        ],
    )?;
    Ok(())
}

/// A window from three columns starting at `first`; incomplete means none.
fn plan_window(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Option<PlanWindow>> {
    let permille: Option<u16> = row.get(first)?;
    let minutes: Option<u32> = row.get(first + 1)?;
    let resets: Option<i64> = row.get(first + 2)?;
    let (Some(used_permille), Some(resets)) = (permille, resets) else {
        return Ok(None);
    };
    Ok(Some(PlanWindow {
        minutes,
        used_permille,
        resets_at: Timestamp::from_millisecond(resets).map_err(|e| conversion(first + 2, e))?,
    }))
}

/// SQLite stores integers as `i64`; token counts never get near the limit.
fn int(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn get_u64(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(column)?).map_err(|e| conversion(column, e))
}

impl TelemetryStore for SqliteStore {
    async fn save_points(&self, points: Vec<TelemetryPoint>) -> Result<(), StoreError> {
        self.call(move |db| {
            let transaction = db.transaction()?;
            {
                let mut insert = transaction.prepare(
                    "INSERT INTO otel_metric (at_ms, metric, detail, model, session, value)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )?;
                for point in &points {
                    insert.execute(params![
                        point.at.as_millisecond(),
                        metric_str(point.metric),
                        point.detail,
                        point.model.as_ref().map(ModelId::as_str),
                        point.session,
                        int(point.value),
                    ])?;
                }
            }
            transaction.commit()
        })
        .await
    }

    async fn totals(&self, from: Timestamp, to: Timestamp) -> Result<TelemetryTotals, StoreError> {
        self.call(move |db| {
            let mut query = db.prepare(
                "SELECT metric, detail, sum(value) FROM otel_metric
                 WHERE at_ms >= ?1 AND at_ms < ?2 GROUP BY metric, detail",
            )?;
            let rows =
                query.query_map(params![from.as_millisecond(), to.as_millisecond()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        get_u64(row, 2)?,
                    ))
                })?;
            let mut totals = TelemetryTotals::default();
            for row in rows {
                let (metric, detail, value) = row?;
                let Some(metric) = parse_metric(&metric) else {
                    continue;
                };
                totals.add(&TelemetryPoint {
                    at: from,
                    metric,
                    detail,
                    model: None,
                    session: None,
                    value,
                });
            }
            Ok(totals)
        })
        .await
    }
}

fn metric_str(metric: TelemetryMetric) -> &'static str {
    match metric {
        TelemetryMetric::Cost => "cost",
        TelemetryMetric::Tokens => "tokens",
        TelemetryMetric::ActiveTime => "active_time",
        TelemetryMetric::LinesAdded => "lines_added",
        TelemetryMetric::LinesRemoved => "lines_removed",
        TelemetryMetric::Commits => "commits",
        TelemetryMetric::PullRequests => "pull_requests",
        TelemetryMetric::Sessions => "sessions",
    }
}

/// Unknown names come from a newer hud and are skipped, not an error.
fn parse_metric(text: &str) -> Option<TelemetryMetric> {
    Some(match text {
        "cost" => TelemetryMetric::Cost,
        "tokens" => TelemetryMetric::Tokens,
        "active_time" => TelemetryMetric::ActiveTime,
        "lines_added" => TelemetryMetric::LinesAdded,
        "lines_removed" => TelemetryMetric::LinesRemoved,
        "commits" => TelemetryMetric::Commits,
        "pull_requests" => TelemetryMetric::PullRequests,
        "sessions" => TelemetryMetric::Sessions,
        _ => return None,
    })
}

fn provider_str(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    }
}

fn parse_provider(text: &str, column: usize) -> rusqlite::Result<Provider> {
    match text {
        "claude" => Ok(Provider::Claude),
        "codex" => Ok(Provider::Codex),
        other => Err(conversion(column, UnknownValue(other.to_owned()))),
    }
}

fn speed_str(speed: Speed) -> &'static str {
    match speed {
        Speed::Standard => "standard",
        Speed::Fast => "fast",
    }
}

fn geo_str(geo: Geo) -> &'static str {
    match geo {
        Geo::Global => "global",
        Geo::Us => "us",
    }
}

fn source_str(source: SourceId) -> &'static str {
    match source {
        SourceId::Homebrew => "homebrew",
        SourceId::Npm => "npm",
        SourceId::Skills => "skills",
    }
}

fn result_str(result: InstallResult) -> &'static str {
    match result {
        InstallResult::Installed => "installed",
        InstallResult::NeedsInteraction => "needs_interaction",
        InstallResult::Failed => "failed",
    }
}

fn kind_str(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::Fetch => "fetch",
        ActionKind::FastForward => "fast_forward",
        ActionKind::Install => "install",
    }
}

fn trigger_str(trigger: Trigger) -> &'static str {
    match trigger {
        Trigger::Manual => "manual",
        Trigger::Auto => "auto",
    }
}

fn operation_str(operation: GitOperation) -> &'static str {
    match operation {
        GitOperation::Merge => "merge",
        GitOperation::Rebase => "rebase",
        GitOperation::CherryPick => "cherry_pick",
        GitOperation::Revert => "revert",
        GitOperation::Bisect => "bisect",
    }
}

fn outcome_columns(outcome: &Outcome) -> (&'static str, Option<&str>) {
    match outcome {
        Outcome::Succeeded => ("succeeded", None),
        Outcome::Failed(message) => ("failed", Some(message)),
        Outcome::Skipped(reason) => ("skipped", Some(reason)),
        Outcome::Cancelled => ("cancelled", None),
        Outcome::TimedOut => ("timed_out", None),
    }
}

fn conversion(
    column: usize,
    source: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(source))
}

#[derive(Debug, thiserror::Error)]
#[error("unknown value {0:?}")]
struct UnknownValue(String);

fn parse_ts(text: &str, column: usize) -> rusqlite::Result<Timestamp> {
    Timestamp::from_str(text).map_err(|e| conversion(column, e))
}

fn parse_commit(text: &str, column: usize) -> rusqlite::Result<CommitId> {
    CommitId::try_from(text).map_err(|e| conversion(column, e))
}

fn parse_kind(text: &str, column: usize) -> rusqlite::Result<ActionKind> {
    match text {
        "fetch" => Ok(ActionKind::Fetch),
        "fast_forward" => Ok(ActionKind::FastForward),
        "install" => Ok(ActionKind::Install),
        other => Err(conversion(column, UnknownValue(other.to_owned()))),
    }
}

fn parse_trigger(text: &str, column: usize) -> rusqlite::Result<Trigger> {
    match text {
        "manual" => Ok(Trigger::Manual),
        "auto" => Ok(Trigger::Auto),
        other => Err(conversion(column, UnknownValue(other.to_owned()))),
    }
}

fn parse_operation(text: &str, column: usize) -> rusqlite::Result<GitOperation> {
    match text {
        "merge" => Ok(GitOperation::Merge),
        "rebase" => Ok(GitOperation::Rebase),
        "cherry_pick" => Ok(GitOperation::CherryPick),
        "revert" => Ok(GitOperation::Revert),
        "bisect" => Ok(GitOperation::Bisect),
        other => Err(conversion(column, UnknownValue(other.to_owned()))),
    }
}

fn parse_outcome(name: &str, detail: Option<String>, column: usize) -> rusqlite::Result<Outcome> {
    match name {
        "succeeded" => Ok(Outcome::Succeeded),
        "failed" => Ok(Outcome::Failed(detail.unwrap_or_default())),
        "skipped" => Ok(Outcome::Skipped(detail.unwrap_or_default())),
        "cancelled" => Ok(Outcome::Cancelled),
        "timed_out" => Ok(Outcome::TimedOut),
        other => Err(conversion(column, UnknownValue(other.to_owned()))),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn ts(text: &str) -> Timestamp {
        text.parse().expect("BUG: valid timestamp")
    }

    fn repo(path: &str) -> RepoPath {
        RepoPath::try_from(PathBuf::from(path)).expect("BUG: absolute")
    }

    fn status() -> RepoStatus {
        RepoStatus {
            head: Head::Branch(BranchName::try_from("main".to_owned()).expect("BUG")),
            head_oid: Some(CommitId::try_from("1".repeat(40).as_str()).expect("BUG")),
            upstream: Some(Upstream {
                name: "origin/main".into(),
                oid: CommitId::try_from("2".repeat(40).as_str()).expect("BUG"),
            }),
            ahead: 0,
            behind: 3,
            tracked_changes: 1,
            untracked: 2,
            operation: Some(GitOperation::Rebase),
        }
    }

    #[test]
    fn migrations_are_valid() {
        migrations()
            .validate()
            .expect("BUG: migrations must validate");
    }

    #[tokio::test]
    async fn action_lifecycle_is_recorded() {
        let store = SqliteStore::in_memory(ts("2026-10-17T10:00:00Z")).expect("BUG");
        let id = store
            .start(
                ts("2026-10-17T10:00:00Z"),
                ActionKind::FastForward,
                "/repos/api",
                Trigger::Auto,
            )
            .await
            .expect("BUG");
        store
            .finish(
                id,
                ts("2026-10-17T10:00:01Z"),
                Outcome::Succeeded,
                Some("3 commits".into()),
            )
            .await
            .expect("BUG");
        let records = store.recent(10).await.expect("BUG");
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.kind, ActionKind::FastForward);
        assert_eq!(record.trigger, Trigger::Auto);
        assert_eq!(record.outcome, Some(Outcome::Succeeded));
        assert_eq!(record.summary.as_deref(), Some("3 commits"));
        assert_eq!(record.finished_at, Some(ts("2026-10-17T10:00:01Z")));
    }

    #[tokio::test]
    async fn failure_detail_survives_a_round_trip() {
        let store = SqliteStore::in_memory(ts("2026-10-17T10:00:00Z")).expect("BUG");
        let id = store
            .start(
                ts("2026-10-17T10:00:00Z"),
                ActionKind::Fetch,
                "/r",
                Trigger::Manual,
            )
            .await
            .expect("BUG");
        store
            .finish(
                id,
                ts("2026-10-17T10:00:02Z"),
                Outcome::Failed("timeout".into()),
                None,
            )
            .await
            .expect("BUG");
        let records = store.recent(1).await.expect("BUG");
        assert_eq!(records[0].outcome, Some(Outcome::Failed("timeout".into())));
    }

    #[tokio::test]
    async fn old_log_entries_are_pruned_on_open() {
        let dir = tempfile::tempdir().expect("BUG");
        let path = dir.path().join("hud.db");
        {
            let store = SqliteStore::open(&path, ts("2026-01-01T00:00:00Z")).expect("BUG");
            store
                .start(
                    ts("2026-01-01T00:00:00Z"),
                    ActionKind::Fetch,
                    "/old",
                    Trigger::Auto,
                )
                .await
                .expect("BUG");
            store
                .start(
                    ts("2026-10-01T00:00:00Z"),
                    ActionKind::Fetch,
                    "/new",
                    Trigger::Auto,
                )
                .await
                .expect("BUG");
        }
        let store = SqliteStore::open(&path, ts("2026-10-17T00:00:00Z")).expect("BUG");
        let targets: Vec<_> = store
            .recent(10)
            .await
            .expect("BUG")
            .into_iter()
            .map(|r| r.target)
            .collect();
        assert_eq!(targets, vec!["/new".to_owned()]);
    }

    #[tokio::test]
    async fn cached_status_round_trips_and_retain_drops_missing_repos() {
        let store = SqliteStore::in_memory(ts("2026-10-17T10:00:00Z")).expect("BUG");
        for path in ["/repos/a", "/repos/b"] {
            ScanCache::save(
                &store,
                CachedRepo {
                    repo: repo(path),
                    status: status(),
                    scanned_at: ts("2026-10-17T10:00:00Z"),
                },
            )
            .await
            .expect("BUG");
        }
        store.retain(vec![repo("/repos/b")]).await.expect("BUG");
        let cached = store.load_all().await.expect("BUG");
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].repo, repo("/repos/b"));
        assert_eq!(cached[0].status, status());
    }

    #[tokio::test]
    async fn install_history_remembers_the_latest_result_per_package() {
        use hud_domain::update::{PackageKind, Version};
        let store = SqliteStore::in_memory(ts("2026-10-17T10:00:00Z")).expect("BUG");
        let firefox = UpdateCandidate {
            source: SourceId::Homebrew,
            kind: PackageKind::Cask,
            name: PackageName::try_from("firefox".to_owned()).expect("BUG"),
            current: Version::new("118.0"),
            latest: Version::new("119.0"),
            pinned: false,
        };
        let at = ts("2026-10-17T10:00:00Z");
        let asked = || store.needed_interaction(SourceId::Homebrew, &firefox.name);
        assert!(!asked().await.expect("BUG"));
        store
            .record(at, &firefox, InstallResult::NeedsInteraction)
            .await
            .expect("BUG");
        assert!(asked().await.expect("BUG"));
        store
            .record(at, &firefox, InstallResult::Installed)
            .await
            .expect("BUG");
        assert!(!asked().await.expect("BUG"));
    }

    #[tokio::test]
    async fn usage_is_merged_by_key_and_cursors_are_replaced() {
        let store = SqliteStore::in_memory(ts("2026-10-17T10:00:00Z")).expect("BUG");
        let record = |output| UsageRecord {
            provider: Provider::Claude,
            key: "msg_a:req_a".into(),
            session: "s".into(),
            model: ModelId::try_from("claude-opus-5-5".to_owned()).expect("BUG"),
            at: ts("2026-10-17T08:00:00Z"),
            tokens: TokenUsage {
                output,
                cache_read: 100,
                ..TokenUsage::default()
            },
            speed: Speed::Fast,
            geo: Geo::Global,
            project: Some("hud".into()),
            subagent: true,
            thinking: 30,
            context_window: None,
        };
        let tool_call = |id: &str| hud_domain::usage::ToolCall {
            provider: Provider::Claude,
            id: id.to_owned(),
            at: ts("2026-10-17T08:00:00Z"),
        };
        let cursor = |offset| FileCursor {
            size: offset,
            offset,
            unreadable: 1,
        };
        UsageStore::save(
            &store,
            Provider::Claude,
            UsageBatch {
                records: vec![record(40), record(120)],
                tool_calls: vec![
                    tool_call("toolu_1"),
                    tool_call("toolu_1"),
                    tool_call("toolu_2"),
                ],
                cursors: Cursors::from([("a.jsonl".to_owned(), cursor(10))]),
                plan: None,
            },
        )
        .await
        .expect("BUG");
        UsageStore::save(
            &store,
            Provider::Claude,
            UsageBatch {
                records: vec![record(90)],
                tool_calls: vec![tool_call("toolu_2")],
                cursors: Cursors::from([("a.jsonl".to_owned(), cursor(20))]),
                plan: None,
            },
        )
        .await
        .expect("BUG");

        let records = store
            .records(ts("2026-10-17T00:00:00Z"), ts("2026-10-18T00:00:00Z"))
            .await
            .expect("BUG");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tokens.output, 120, "the highest count wins");
        assert_eq!(records[0].speed, Speed::Fast);
        assert_eq!(records[0].project.as_deref(), Some("hud"));
        assert!(records[0].subagent);
        assert_eq!(records[0].thinking, 30);
        let calls = store
            .tool_calls(
                Provider::Claude,
                ts("2026-10-17T00:00:00Z"),
                ts("2026-10-18T00:00:00Z"),
            )
            .await
            .expect("BUG");
        assert_eq!(calls, 2, "repeated ids count once");
        let codex_calls = store
            .tool_calls(
                Provider::Codex,
                ts("2026-10-17T00:00:00Z"),
                ts("2026-10-18T00:00:00Z"),
            )
            .await
            .expect("BUG");
        assert_eq!(codex_calls, 0, "tool calls are counted per provider");
        let times = store
            .answer_times(ts("2026-10-17T00:00:00Z"), ts("2026-10-18T00:00:00Z"))
            .await
            .expect("BUG");
        assert_eq!(times, vec![ts("2026-10-17T08:00:00Z")]);
        let cursors = store.cursors(Provider::Claude).await.expect("BUG");
        assert_eq!(cursors["a.jsonl"].offset, 20);
        assert!(
            store
                .records(ts("2026-10-17T09:00:00Z"), ts("2026-10-18T00:00:00Z"))
                .await
                .expect("BUG")
                .is_empty(),
            "the range end is exclusive and filters by time"
        );
    }

    #[tokio::test]
    async fn only_newer_plan_usage_replaces_the_stored_one() {
        let store = SqliteStore::in_memory(ts("2026-10-17T10:00:00Z")).expect("BUG");
        let plan = |at: &str, permille| PlanUsage {
            at: ts(at),
            plan: Some("plus".into()),
            primary: Some(PlanWindow {
                minutes: Some(300),
                used_permille: permille,
                resets_at: ts("2026-10-17T12:00:00Z"),
            }),
            secondary: None,
        };
        let save = |plan| {
            UsageStore::save(
                &store,
                Provider::Codex,
                UsageBatch {
                    plan: Some(plan),
                    ..UsageBatch::default()
                },
            )
        };
        assert_eq!(store.plan(Provider::Codex).await.expect("BUG"), None);
        save(plan("2026-10-17T09:00:00Z", 120)).await.expect("BUG");
        save(plan("2026-10-17T08:00:00Z", 50)).await.expect("BUG");
        let stored = store.plan(Provider::Codex).await.expect("BUG");
        assert_eq!(stored, Some(plan("2026-10-17T09:00:00Z", 120)));
        assert_eq!(store.plan(Provider::Claude).await.expect("BUG"), None);
    }

    #[tokio::test]
    async fn telemetry_totals_sum_per_metric_within_the_range() {
        let store = SqliteStore::in_memory(ts("2026-10-17T10:00:00Z")).expect("BUG");
        let point = |at: &str, metric, detail: Option<&str>, value| TelemetryPoint {
            at: ts(at),
            metric,
            detail: detail.map(str::to_owned),
            model: None,
            session: Some("s".into()),
            value,
        };
        store
            .save_points(vec![
                point("2026-10-17T08:00:00Z", TelemetryMetric::Cost, None, 120_000),
                point("2026-10-17T09:00:00Z", TelemetryMetric::Cost, None, 30_000),
                point(
                    "2026-10-17T09:00:00Z",
                    TelemetryMetric::ActiveTime,
                    Some("user"),
                    5_000,
                ),
                point(
                    "2026-10-17T09:00:00Z",
                    TelemetryMetric::ActiveTime,
                    Some("cli"),
                    7_000,
                ),
                point(
                    "2026-10-17T09:00:00Z",
                    TelemetryMetric::LinesAdded,
                    Some("added"),
                    42,
                ),
                point("2026-10-16T09:00:00Z", TelemetryMetric::Commits, None, 9),
            ])
            .await
            .expect("BUG");
        let totals = store
            .totals(ts("2026-10-17T00:00:00Z"), ts("2026-10-18T00:00:00Z"))
            .await
            .expect("BUG");
        assert_eq!(totals.cost_micro_usd, 150_000);
        assert_eq!(totals.active_user_ms, 5_000);
        assert_eq!(totals.active_cli_ms, 7_000);
        assert_eq!(totals.lines_added, 42);
        assert_eq!(totals.commits, 0, "yesterday is outside the range");
    }
}
