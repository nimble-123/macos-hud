//! Messages between the presentation layer and the use cases.

use hud_domain::git::RepoPath;

use crate::usecases::workspaces::{RepoReport, WorkspaceReport};

/// Where a workspace report comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportSource {
    /// Last known state from the scan cache.
    Cache,
    /// Local scan without contacting remotes.
    LocalScan,
    /// Fetch, scan and automatic fast-forwards.
    Sync,
}

/// Result of background work, sent from use cases to the presentation layer.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// A complete picture of all workspaces.
    Workspaces {
        /// Reports in configured order.
        reports: Vec<WorkspaceReport>,
        /// How they were obtained.
        source: ReportSource,
    },
    /// A sync with fetch started.
    SyncStarted,
    /// One repository changed through a manual action.
    RepoUpdated(RepoReport),
}

/// Request from the presentation layer to run a use case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Fetch and sync all workspaces now.
    SyncNow,
    /// Fast-forward one repository on request of the user.
    FastForward(RepoPath),
}
