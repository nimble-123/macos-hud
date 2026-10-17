//! Package managers as sources of updates.

use futures::future::BoxFuture;
use hud_domain::update::{InstallPermit, SourceId, UpdateCandidate};

/// Whether a source's command-line tool can be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// Installed; the version as the tool reports it.
    Available(String),
    /// Not found in the PATH.
    Missing,
    /// Found but not working.
    Broken(String),
}

/// Result of checking a source.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceCheck {
    /// Available updates; empty when the source cannot tell (see `updates_known`).
    pub candidates: Vec<UpdateCandidate>,
    /// Installed items, for sources that list what they manage (Skills).
    pub installed: Vec<String>,
    /// False when the source cannot know about updates without installing them.
    pub updates_known: bool,
}

/// How an installation ended when the tool ran to completion.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum InstallOutcome {
    /// Installed.
    Installed,
    /// The tool asked for a password or other input; nothing was installed.
    NeedsInteraction,
}

/// Why talking to a source failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    /// The tool is not installed.
    #[error("{0} not found")]
    Missing(&'static str),
    /// The tool did not finish in time.
    #[error("{0} timed out")]
    TimedOut(&'static str),
    /// hud is quitting and stopped the tool.
    #[error("{0} was cancelled")]
    Cancelled(&'static str),
    /// The tool failed; the end of its output explains why.
    #[error("{tool} failed: {message}")]
    Failed {
        /// Tool name.
        tool: &'static str,
        /// End of its error output.
        message: String,
    },
    /// The tool's output could not be understood.
    #[error("unexpected output from {tool}: {message}")]
    Unparsable {
        /// Tool name.
        tool: &'static str,
        /// What was wrong.
        message: String,
    },
    /// The permit is for something this source does not install.
    #[error("{0} cannot install this target")]
    WrongTarget(&'static str),
}

/// A package manager that can report and install updates.
///
/// Dyn-compatible (methods return `BoxFuture`) because all sources are held
/// in one list (ADR 0002).
pub trait UpdateSource: Send + Sync {
    /// Which source this is.
    fn id(&self) -> SourceId;

    /// Whether the tool can be used, with its version.
    fn availability(&self) -> BoxFuture<'_, Availability>;

    /// Refreshes the package index if needed and lists available updates.
    fn check(&self) -> BoxFuture<'_, Result<SourceCheck, UpdateError>>;

    /// Installs what the permit allows; consumes it.
    fn install(&self, permit: InstallPermit) -> BoxFuture<'_, Result<InstallOutcome, UpdateError>>;
}
