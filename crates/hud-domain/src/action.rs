//! Log of actions hud performed, manually or automatically.

use jiff::Timestamp;

/// Database id of a logged action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(pub i64);

/// What was done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// `git fetch`.
    Fetch,
    /// Fast-forward of a branch.
    FastForward,
    /// Installation of a package update.
    Install,
}

/// Who started the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The user, by key or click.
    Manual,
    /// The scheduler.
    Auto,
}

/// How the action ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Done.
    Succeeded,
    /// Failed with a message.
    Failed(String),
    /// Not done, with the reason.
    Skipped(String),
    /// Stopped by the user.
    Cancelled,
    /// Took too long and was stopped.
    TimedOut,
}

/// One entry of the action log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRecord {
    /// Id.
    pub id: ActionId,
    /// Start time.
    pub started_at: Timestamp,
    /// End time; `None` while running.
    pub finished_at: Option<Timestamp>,
    /// What was done.
    pub kind: ActionKind,
    /// What it was done to, e.g. a repository path.
    pub target: String,
    /// Who started it.
    pub trigger: Trigger,
    /// Result; `None` while running.
    pub outcome: Option<Outcome>,
    /// Summary such as "3 commits".
    pub summary: Option<String>,
}
