//! SQLite persistence behind a dedicated actor thread (ADR 0006).

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use hud_application::ports::store::{ActionLog, CachedRepo, ScanCache, StoreError};
use hud_domain::action::{ActionId, ActionKind, ActionRecord, Outcome, Trigger};
use hud_domain::git::{BranchName, CommitId, GitOperation, Head, RepoPath, RepoStatus, Upstream};
use jiff::Timestamp;
use rusqlite::{Connection, params};
use rusqlite_migration::{M, Migrations};
use tokio::sync::{mpsc, oneshot};

/// Queue length towards the DB thread; callers wait when it is full.
const QUEUE: usize = 64;
/// Log entries older than this are deleted at start-up.
const ACTION_LOG_RETENTION: Duration = Duration::from_hours(2160);

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
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
    )])
}

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

fn kind_str(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::Fetch => "fetch",
        ActionKind::FastForward => "fast_forward",
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
            store
                .save(CachedRepo {
                    repo: repo(path),
                    status: status(),
                    scanned_at: ts("2026-10-17T10:00:00Z"),
                })
                .await
                .expect("BUG");
        }
        store.retain(vec![repo("/repos/b")]).await.expect("BUG");
        let cached = store.load_all().await.expect("BUG");
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].repo, repo("/repos/b"));
        assert_eq!(cached[0].status, status());
    }
}
