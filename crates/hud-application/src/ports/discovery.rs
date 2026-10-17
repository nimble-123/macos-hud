//! Finding repositories below workspace folders.

use std::future::Future;

use hud_domain::WorkspacePath;
use hud_domain::git::RepoPath;

use crate::settings::ScanSettings;

/// Why a workspace could not be searched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryError {
    /// The folder does not exist, e.g. an unmounted volume.
    #[error("workspace not available: {0}")]
    Unavailable(String),
    /// The folder exists but cannot be read.
    #[error("cannot read workspace {path}: {message}")]
    Unreadable {
        /// Folder.
        path: String,
        /// Reason.
        message: String,
    },
}

/// Lists Git repositories below a folder.
pub trait RepoDiscovery: Send + Sync {
    /// Repositories below `workspace`, sorted by path.
    fn discover(
        &self,
        workspace: &WorkspacePath,
        scan: &ScanSettings,
    ) -> impl Future<Output = Result<Vec<RepoPath>, DiscoveryError>> + Send;
}
