//! Messages between the presentation layer and the use cases.

use hud_domain::WorkspacePath;
use hud_domain::git::RepoPath;

use hud_domain::update::InstallTarget;

use crate::usecases::updates::{JobUpdate, SourceReport};
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
    /// A scan is about to read `total` repositories.
    ScanStarted {
        /// Repositories that will be read.
        total: usize,
        /// Whether remotes are fetched first.
        fetch: bool,
    },
    /// One repository is done; sent while a scan is still running.
    RepoScanned {
        /// Workspace the repository belongs to.
        workspace: WorkspacePath,
        /// Its result.
        report: RepoReport,
    },
    /// One repository changed through a manual action.
    RepoUpdated(RepoReport),
    /// A complete picture of all update sources.
    Updates(Vec<SourceReport>),
    /// One source was checked again, e.g. after an installation.
    SourceUpdated(SourceReport),
    /// An installation job changed state.
    Job(JobUpdate),
}

/// Request from the presentation layer to run a use case, grouped by the
/// agent that handles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// For the workspace agent.
    Workspaces(WorkspaceCommand),
    /// For the updates agent.
    Updates(UpdateCommand),
}

/// Workspace requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceCommand {
    /// Fetch and sync all workspaces now.
    SyncNow,
    /// Fast-forward one repository on request of the user.
    FastForward(RepoPath),
}

/// Update requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCommand {
    /// Check all sources now.
    CheckNow,
    /// Install an update.
    Install {
        /// What to install.
        target: InstallTarget,
        /// The user confirmed the system-wide change in a dialog.
        confirmed: bool,
    },
}
