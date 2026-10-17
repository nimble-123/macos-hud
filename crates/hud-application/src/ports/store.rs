//! Persistence ports: action log and scan cache.

use std::future::Future;
use std::sync::Arc;

use hud_domain::action::{ActionId, ActionKind, ActionRecord, Outcome, Trigger};
use hud_domain::git::{RepoPath, RepoStatus};
use jiff::Timestamp;

/// Why the store could not be used.
#[derive(Debug, thiserror::Error)]
#[error("store error: {message}")]
pub struct StoreError {
    /// What went wrong.
    pub message: String,
}

/// Records every action before it starts and after it ends.
pub trait ActionLog: Send + Sync {
    /// Logs the start of an action and returns its id.
    fn start(
        &self,
        at: Timestamp,
        kind: ActionKind,
        target: &str,
        trigger: Trigger,
    ) -> impl Future<Output = Result<ActionId, StoreError>> + Send;

    /// Logs the end of an action.
    fn finish(
        &self,
        id: ActionId,
        at: Timestamp,
        outcome: Outcome,
        summary: Option<String>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Most recent actions first.
    fn recent(
        &self,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<ActionRecord>, StoreError>> + Send;
}

/// Last known status of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedRepo {
    /// Repository.
    pub repo: RepoPath,
    /// Status at `scanned_at`; decisions are recomputed from it.
    pub status: RepoStatus,
    /// When the status was read.
    pub scanned_at: Timestamp,
}

/// Keeps the last scan so the UI can show something before the first scan ends.
pub trait ScanCache: Send + Sync {
    /// Replaces the cached status of one repository.
    fn save(&self, entry: CachedRepo) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// All cached repositories, ordered by path.
    fn load_all(&self) -> impl Future<Output = Result<Vec<CachedRepo>, StoreError>> + Send;

    /// Drops repositories that no longer exist under any workspace.
    fn retain(&self, repos: Vec<RepoPath>) -> impl Future<Output = Result<(), StoreError>> + Send;
}

/// Current time; a port so use cases are testable with a fixed clock.
pub trait Clock: Send + Sync {
    /// Now.
    fn now(&self) -> Timestamp;
}

impl<T: ActionLog> ActionLog for Arc<T> {
    fn start(
        &self,
        at: Timestamp,
        kind: ActionKind,
        target: &str,
        trigger: Trigger,
    ) -> impl Future<Output = Result<ActionId, StoreError>> + Send {
        (**self).start(at, kind, target, trigger)
    }

    fn finish(
        &self,
        id: ActionId,
        at: Timestamp,
        outcome: Outcome,
        summary: Option<String>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).finish(id, at, outcome, summary)
    }

    fn recent(
        &self,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<ActionRecord>, StoreError>> + Send {
        (**self).recent(limit)
    }
}

impl<T: ScanCache> ScanCache for Arc<T> {
    fn save(&self, entry: CachedRepo) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).save(entry)
    }

    fn load_all(&self) -> impl Future<Output = Result<Vec<CachedRepo>, StoreError>> + Send {
        (**self).load_all()
    }

    fn retain(&self, repos: Vec<RepoPath>) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).retain(repos)
    }
}
