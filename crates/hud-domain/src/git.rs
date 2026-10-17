//! Git repository state and the fast-forward rule.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::DomainError;

/// Absolute path of a repository's working tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoPath(PathBuf);

impl RepoPath {
    /// The path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl TryFrom<PathBuf> for RepoPath {
    type Error = DomainError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        if path.is_absolute() {
            Ok(Self(path))
        } else {
            Err(DomainError::RelativeRepoPath(path))
        }
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

/// Name of a local branch.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchName(String);

impl BranchName {
    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BranchName {
    type Error = DomainError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            Err(DomainError::InvalidBranchName(name))
        } else {
            Ok(Self(name))
        }
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Full object id: 40 hex digits (SHA-1) or 64 (SHA-256), stored lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitId(String);

impl CommitId {
    /// The id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for CommitId {
    type Error = DomainError;

    fn try_from(id: &str) -> Result<Self, Self::Error> {
        let valid_length = id.len() == 40 || id.len() == 64;
        if valid_length && id.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(Self(id.to_ascii_lowercase()))
        } else {
            Err(DomainError::InvalidCommitId(id.to_owned()))
        }
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What `HEAD` points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// A local branch.
    Branch(BranchName),
    /// A commit, not a branch.
    Detached,
}

/// Remote-tracking branch of the current branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    /// Name such as `origin/main`.
    pub name: String,
    /// Commit the upstream points to.
    pub oid: CommitId,
}

/// A multi-step Git operation that is not finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOperation {
    /// `git merge`.
    Merge,
    /// `git rebase`.
    Rebase,
    /// `git cherry-pick`.
    CherryPick,
    /// `git revert`.
    Revert,
    /// `git bisect`.
    Bisect,
}

/// Snapshot of a repository as reported by Git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus {
    /// What `HEAD` points to.
    pub head: Head,
    /// Commit of `HEAD`; `None` on a branch without commits.
    pub head_oid: Option<CommitId>,
    /// Upstream of the current branch.
    pub upstream: Option<Upstream>,
    /// Commits on the branch that the upstream does not have.
    pub ahead: u32,
    /// Commits on the upstream that the branch does not have.
    pub behind: u32,
    /// Modified, staged or conflicted tracked files.
    pub tracked_changes: u32,
    /// Untracked files; shown, never blocking (ADR 0010).
    pub untracked: u32,
    /// Unfinished operation, if any.
    pub operation: Option<GitOperation>,
}

/// Who asks for the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// The scheduler; respects the workspace opt-out.
    Auto {
        /// The workspace has switched automatic fast-forward off.
        opted_out: bool,
    },
    /// The user asked explicitly; the opt-out does not apply, every safety rule does.
    Manual,
}

/// Why a repository is not fast-forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// A merge, rebase, cherry-pick, revert or bisect is unfinished.
    OperationInProgress(GitOperation),
    /// No branch is checked out.
    DetachedHead,
    /// The branch has no commits yet.
    NoCommits,
    /// The branch has no upstream.
    NoUpstream,
    /// Tracked files are modified or staged.
    TrackedChanges,
    /// Branch and upstream both have commits the other lacks.
    Diverged {
        /// Local-only commits.
        ahead: u32,
        /// Upstream-only commits.
        behind: u32,
    },
    /// The branch has local commits and is not behind.
    Ahead(u32),
    /// Fast-forward would be possible, but the workspace opted out of automation.
    OptedOut,
}

/// Proof that [`sync_decision`] allowed a fast-forward of exactly these commits.
///
/// Only this module can create one (private fields, no `Clone`), so a
/// fast-forward without a passed rule check does not compile (ADR 0003).
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a permit that is not used means the fast-forward never happens"]
pub struct FastForwardPermit {
    repo: RepoPath,
    head_oid: CommitId,
    target_oid: CommitId,
    commits: u32,
}

impl FastForwardPermit {
    /// Repository to fast-forward.
    #[must_use]
    pub fn repo(&self) -> &RepoPath {
        &self.repo
    }

    /// `HEAD` at decision time; the adapter refuses if `HEAD` moved since.
    #[must_use]
    pub fn head_oid(&self) -> &CommitId {
        &self.head_oid
    }

    /// Upstream commit to move to.
    #[must_use]
    pub fn target_oid(&self) -> &CommitId {
        &self.target_oid
    }

    /// Number of commits the branch moves forward.
    #[must_use]
    pub fn commits(&self) -> u32 {
        self.commits
    }
}

/// Outcome of [`sync_decision`].
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub enum SyncDecision {
    /// Branch and upstream point to the same commit.
    UpToDate,
    /// All rules passed.
    FastForward(FastForwardPermit),
    /// A rule failed.
    Skip(SkipReason),
}

/// Decides whether a repository may be fast-forwarded.
///
/// The rules run in a fixed order (PRD §4.2, figure 2), so a repository always
/// reports the first reason that blocks it.
pub fn sync_decision(repo: &RepoPath, status: &RepoStatus, mode: SyncMode) -> SyncDecision {
    if let Some(operation) = status.operation {
        return SyncDecision::Skip(SkipReason::OperationInProgress(operation));
    }
    if status.head == Head::Detached {
        return SyncDecision::Skip(SkipReason::DetachedHead);
    }
    let Some(head_oid) = &status.head_oid else {
        return SyncDecision::Skip(SkipReason::NoCommits);
    };
    let Some(upstream) = &status.upstream else {
        return SyncDecision::Skip(SkipReason::NoUpstream);
    };
    if status.tracked_changes > 0 {
        return SyncDecision::Skip(SkipReason::TrackedChanges);
    }
    match (status.ahead, status.behind) {
        (0, 0) => SyncDecision::UpToDate,
        (ahead, 0) => SyncDecision::Skip(SkipReason::Ahead(ahead)),
        (0, _) if mode == (SyncMode::Auto { opted_out: true }) => {
            SyncDecision::Skip(SkipReason::OptedOut)
        }
        (0, behind) => SyncDecision::FastForward(FastForwardPermit {
            repo: repo.clone(),
            head_oid: head_oid.clone(),
            target_oid: upstream.oid.clone(),
            commits: behind,
        }),
        (ahead, behind) => SyncDecision::Skip(SkipReason::Diverged { ahead, behind }),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const HEAD: &str = "1111111111111111111111111111111111111111";
    const UPSTREAM: &str = "2222222222222222222222222222222222222222";
    const AUTO: SyncMode = SyncMode::Auto { opted_out: false };

    fn repo() -> RepoPath {
        RepoPath::try_from(PathBuf::from("/repos/api")).expect("BUG: absolute path")
    }

    /// Clean branch three commits behind its upstream: the fast-forward case.
    fn behind_by_three() -> RepoStatus {
        RepoStatus {
            head: Head::Branch(BranchName::try_from("main".to_owned()).expect("BUG: valid name")),
            head_oid: Some(CommitId::try_from(HEAD).expect("BUG: valid id")),
            upstream: Some(Upstream {
                name: "origin/main".into(),
                oid: CommitId::try_from(UPSTREAM).expect("BUG: valid id"),
            }),
            ahead: 0,
            behind: 3,
            tracked_changes: 0,
            untracked: 0,
            operation: None,
        }
    }

    #[test]
    fn clean_branch_behind_upstream_gets_a_permit_for_both_commits() {
        let SyncDecision::FastForward(permit) = sync_decision(&repo(), &behind_by_three(), AUTO)
        else {
            panic!("expected a permit");
        };
        assert_eq!(permit.head_oid().as_str(), HEAD);
        assert_eq!(permit.target_oid().as_str(), UPSTREAM);
        assert_eq!(permit.commits(), 3);
        assert_eq!(permit.repo(), &repo());
    }

    #[test]
    fn sync_decision_skips_when_tracked_changes() {
        let status = RepoStatus {
            tracked_changes: 1,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::Skip(SkipReason::TrackedChanges)
        );
    }

    #[test]
    fn untracked_files_do_not_block() {
        let status = RepoStatus {
            untracked: 5,
            ..behind_by_three()
        };
        assert!(matches!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::FastForward(_)
        ));
    }

    #[test]
    fn running_operation_wins_over_every_other_reason() {
        let status = RepoStatus {
            operation: Some(GitOperation::Rebase),
            head: Head::Detached,
            upstream: None,
            tracked_changes: 3,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::Skip(SkipReason::OperationInProgress(GitOperation::Rebase))
        );
    }

    #[test]
    fn detached_head_is_skipped() {
        let status = RepoStatus {
            head: Head::Detached,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::Skip(SkipReason::DetachedHead)
        );
    }

    #[test]
    fn branch_without_commits_is_skipped() {
        let status = RepoStatus {
            head_oid: None,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::Skip(SkipReason::NoCommits)
        );
    }

    #[test]
    fn missing_upstream_is_skipped() {
        let status = RepoStatus {
            upstream: None,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::Skip(SkipReason::NoUpstream)
        );
    }

    #[test]
    fn diverged_history_reports_both_counts() {
        let status = RepoStatus {
            ahead: 2,
            behind: 5,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::Skip(SkipReason::Diverged {
                ahead: 2,
                behind: 5
            })
        );
    }

    #[test]
    fn ahead_only_is_skipped() {
        let status = RepoStatus {
            ahead: 1,
            behind: 0,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::Skip(SkipReason::Ahead(1))
        );
    }

    #[test]
    fn equal_commits_are_up_to_date() {
        let status = RepoStatus {
            behind: 0,
            ..behind_by_three()
        };
        assert_eq!(
            sync_decision(&repo(), &status, AUTO),
            SyncDecision::UpToDate
        );
    }

    #[test]
    fn opted_out_workspace_is_skipped_in_auto_mode() {
        let decision = sync_decision(
            &repo(),
            &behind_by_three(),
            SyncMode::Auto { opted_out: true },
        );
        assert_eq!(decision, SyncDecision::Skip(SkipReason::OptedOut));
    }

    #[test]
    fn manual_mode_ignores_the_opt_out() {
        let decision = sync_decision(&repo(), &behind_by_three(), SyncMode::Manual);
        assert!(matches!(decision, SyncDecision::FastForward(_)));
    }

    #[test]
    fn manual_mode_keeps_every_safety_rule() {
        let status = RepoStatus {
            tracked_changes: 1,
            ..behind_by_three()
        };
        let decision = sync_decision(&repo(), &status, SyncMode::Manual);
        assert_eq!(decision, SyncDecision::Skip(SkipReason::TrackedChanges));
    }

    #[test]
    fn commit_id_accepts_sha1_and_sha256_and_normalises_case() {
        assert!(CommitId::try_from("A".repeat(40).as_str()).is_ok());
        assert!(CommitId::try_from("b".repeat(64).as_str()).is_ok());
        assert_eq!(
            CommitId::try_from("A".repeat(40).as_str())
                .expect("BUG")
                .as_str(),
            "a".repeat(40)
        );
    }

    #[test]
    fn commit_id_rejects_short_and_non_hex_values() {
        assert!(CommitId::try_from("abc").is_err());
        assert!(CommitId::try_from("g".repeat(40).as_str()).is_err());
    }

    #[test]
    fn branch_name_rejects_empty_and_whitespace() {
        assert!(BranchName::try_from(String::new()).is_err());
        assert!(BranchName::try_from("my branch".to_owned()).is_err());
    }

    fn any_status() -> impl Strategy<Value = (RepoStatus, SyncMode)> {
        let operation = prop_oneof![
            Just(None),
            Just(Some(GitOperation::Merge)),
            Just(Some(GitOperation::Rebase)),
        ];
        (
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            0u32..4,
            0u32..4,
            0u32..3,
            0u32..3,
            operation,
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(
                |(
                    detached,
                    has_oid,
                    has_upstream,
                    ahead,
                    behind,
                    tracked,
                    untracked,
                    operation,
                    manual,
                    opted_out,
                )| {
                    let base = behind_by_three();
                    let status = RepoStatus {
                        head: if detached { Head::Detached } else { base.head },
                        head_oid: if has_oid { base.head_oid } else { None },
                        upstream: if has_upstream { base.upstream } else { None },
                        ahead,
                        behind,
                        tracked_changes: tracked,
                        untracked,
                        operation,
                    };
                    let mode = if manual {
                        SyncMode::Manual
                    } else {
                        SyncMode::Auto { opted_out }
                    };
                    (status, mode)
                },
            )
    }

    proptest! {
        #[test]
        fn never_fast_forwards_with_tracked_changes((status, mode) in any_status()) {
            if status.tracked_changes > 0 {
                prop_assert!(!matches!(sync_decision(&repo(), &status, mode), SyncDecision::FastForward(_)));
            }
        }

        #[test]
        fn never_fast_forwards_with_local_commits((status, mode) in any_status()) {
            if status.ahead > 0 {
                prop_assert!(!matches!(sync_decision(&repo(), &status, mode), SyncDecision::FastForward(_)));
            }
        }

        #[test]
        fn permit_always_moves_by_the_behind_count((status, mode) in any_status()) {
            if let SyncDecision::FastForward(permit) = sync_decision(&repo(), &status, mode) {
                prop_assert_eq!(permit.commits(), status.behind);
                prop_assert!(status.behind > 0);
                prop_assert!(status.operation.is_none());
                prop_assert_eq!(status.head.clone(), behind_by_three().head);
            }
        }

        #[test]
        fn opted_out_auto_mode_never_fast_forwards((status, _mode) in any_status()) {
            let decision = sync_decision(&repo(), &status, SyncMode::Auto { opted_out: true });
            prop_assert!(!matches!(decision, SyncDecision::FastForward(_)));
        }
    }
}
