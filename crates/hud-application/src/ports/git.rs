//! Access to Git repositories.

use std::future::Future;
use std::sync::Arc;

use hud_domain::git::{CommitId, FastForwardPermit, RepoPath, RepoStatus};

/// Result of a fast-forward attempt that reached Git.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum FfOutcome {
    /// The branch moved to the upstream commit.
    Advanced {
        /// Previous `HEAD`.
        from: CommitId,
        /// New `HEAD`.
        to: CommitId,
        /// Number of commits moved forward.
        commits: u32,
    },
    /// `HEAD` moved since the decision; nothing was changed.
    StateChanged,
}

/// Why a Git operation failed.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// `git` is not installed or not in the PATH.
    #[error("git not found")]
    GitMissing,
    /// The command did not finish in time.
    #[error("git {action} timed out")]
    TimedOut {
        /// Sub-command, e.g. `fetch`.
        action: &'static str,
    },
    /// hud is quitting and stopped git.
    #[error("git {action} was cancelled")]
    Cancelled {
        /// Sub-command.
        action: &'static str,
    },
    /// Git exited with an error; `message` is the end of its stderr.
    #[error("git {action} failed: {message}")]
    Failed {
        /// Sub-command.
        action: &'static str,
        /// End of stderr.
        message: String,
    },
    /// Git's output could not be understood.
    #[error("unexpected output from git {action}: {message}")]
    Unparsable {
        /// Sub-command.
        action: &'static str,
        /// What was wrong.
        message: String,
    },
}

/// Reads and updates Git repositories.
pub trait GitPort: Send + Sync {
    /// Current state of the repository; does not contact remotes.
    fn status(&self, repo: &RepoPath) -> impl Future<Output = Result<RepoStatus, GitError>> + Send;

    /// Updates remote-tracking branches without ever asking for credentials.
    fn fetch(&self, repo: &RepoPath) -> impl Future<Output = Result<(), GitError>> + Send;

    /// Moves the branch to the permitted upstream commit. Consumes the permit,
    /// so every fast-forward needs its own passed rule check.
    fn fast_forward(
        &self,
        permit: FastForwardPermit,
    ) -> impl Future<Output = Result<FfOutcome, GitError>> + Send;
}

impl<T: GitPort> GitPort for Arc<T> {
    fn status(&self, repo: &RepoPath) -> impl Future<Output = Result<RepoStatus, GitError>> + Send {
        (**self).status(repo)
    }

    fn fetch(&self, repo: &RepoPath) -> impl Future<Output = Result<(), GitError>> + Send {
        (**self).fetch(repo)
    }

    fn fast_forward(
        &self,
        permit: FastForwardPermit,
    ) -> impl Future<Output = Result<FfOutcome, GitError>> + Send {
        (**self).fast_forward(permit)
    }
}
