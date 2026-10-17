//! [`GitPort`] adapter that runs the `git` CLI (ADR 0005).

use std::path::Path;
use std::time::Duration;

use hud_application::ports::git::{FfOutcome, GitError, GitPort};
use hud_domain::git::{
    BranchName, CommitId, FastForwardPermit, GitOperation, Head, RepoPath, RepoStatus, Upstream,
};

use crate::process::{ProcessError, ProcessOutput, ProcessRunner, ProcessSpec};

const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);
const MERGE_TIMEOUT: Duration = Duration::from_secs(60);

/// Runs `git` through a [`ProcessRunner`].
#[derive(Debug, Clone)]
pub struct GitCli<R> {
    runner: R,
    program: String,
}

impl<R: ProcessRunner> GitCli<R> {
    /// `program` is `git` or an explicit path from the config.
    pub fn new(runner: R, program: impl Into<String>) -> Self {
        Self {
            runner,
            program: program.into(),
        }
    }

    async fn git(
        &self,
        repo: &RepoPath,
        action: &'static str,
        args: &[&str],
        timeout: Duration,
        env: &[(&str, &str)],
    ) -> Result<ProcessOutput, GitError> {
        let mut all = vec!["-C", path_str(repo.as_path(), action)?];
        all.extend_from_slice(args);
        let mut spec = ProcessSpec::new(self.program.clone(), &all, timeout);
        spec.env = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let output = self.runner.run(spec).await.map_err(|error| match error {
            ProcessError::NotFound { .. } => GitError::GitMissing,
            ProcessError::TimedOut { .. } => GitError::TimedOut { action },
            ProcessError::Cancelled { .. } => GitError::Cancelled { action },
            ProcessError::Io { source, .. } => GitError::Failed {
                action,
                message: source.to_string(),
            },
        })?;
        if output.code == Some(0) {
            Ok(output)
        } else {
            Err(GitError::Failed {
                action,
                message: last_line(&output.stderr),
            })
        }
    }
}

impl<R: ProcessRunner> GitPort for GitCli<R> {
    async fn status(&self, repo: &RepoPath) -> Result<RepoStatus, GitError> {
        // Optional locks off: a status read must never block the user's own git.
        let no_locks = [("GIT_OPTIONAL_LOCKS", "0")];
        let output = self
            .git(
                repo,
                "status",
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "--untracked-files=normal",
                ],
                STATUS_TIMEOUT,
                &no_locks,
            )
            .await?;
        let parsed = parse_status(&output.stdout)?;

        let mut rev_args = vec!["rev-parse", "--absolute-git-dir"];
        if parsed.upstream_name.is_some() {
            rev_args.push("@{upstream}");
        }
        let rev = self
            .git(repo, "rev-parse", &rev_args, STATUS_TIMEOUT, &no_locks)
            .await?;
        let mut lines = rev.stdout.lines();
        let git_dir = lines
            .next()
            .ok_or_else(|| unparsable("rev-parse", "missing git dir"))?;
        let upstream = match parsed.upstream_name {
            Some(name) => {
                let oid = lines
                    .next()
                    .ok_or_else(|| unparsable("rev-parse", "missing upstream"))?;
                Some(Upstream {
                    name,
                    oid: commit_id(oid, "rev-parse")?,
                })
            }
            None => None,
        };

        Ok(RepoStatus {
            head: parsed.head,
            head_oid: parsed.head_oid,
            upstream,
            ahead: parsed.ahead,
            behind: parsed.behind,
            tracked_changes: parsed.tracked_changes,
            untracked: parsed.untracked,
            operation: operation_in_progress(Path::new(git_dir)).await,
        })
    }

    async fn fetch(&self, repo: &RepoPath) -> Result<(), GitError> {
        let env = [
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_SSH_COMMAND", "ssh -o BatchMode=yes"),
        ];
        self.git(
            repo,
            "fetch",
            &["fetch", "--quiet", "--prune"],
            FETCH_TIMEOUT,
            &env,
        )
        .await?;
        Ok(())
    }

    async fn fast_forward(&self, permit: FastForwardPermit) -> Result<FfOutcome, GitError> {
        let repo = permit.repo();
        let head = self
            .git(
                repo,
                "rev-parse",
                &["rev-parse", "HEAD"],
                STATUS_TIMEOUT,
                &[],
            )
            .await?;
        if commit_id(head.stdout.trim(), "rev-parse")? != *permit.head_oid() {
            return Ok(FfOutcome::StateChanged);
        }
        // Git itself refuses to touch files with local modifications.
        let target = permit.target_oid().as_str();
        self.git(
            repo,
            "merge",
            &["merge", "--ff-only", "--quiet", target],
            MERGE_TIMEOUT,
            &[],
        )
        .await?;
        Ok(FfOutcome::Advanced {
            from: permit.head_oid().clone(),
            to: permit.target_oid().clone(),
            commits: permit.commits(),
        })
    }
}

/// Fields of `git status --porcelain=v2 --branch`.
#[derive(Debug, PartialEq, Eq)]
struct ParsedStatus {
    head: Head,
    head_oid: Option<CommitId>,
    upstream_name: Option<String>,
    ahead: u32,
    behind: u32,
    tracked_changes: u32,
    untracked: u32,
}

fn parse_status(stdout: &str) -> Result<ParsedStatus, GitError> {
    let mut status = ParsedStatus {
        head: Head::Detached,
        head_oid: None,
        upstream_name: None,
        ahead: 0,
        behind: 0,
        tracked_changes: 0,
        untracked: 0,
    };
    for line in stdout.lines() {
        if let Some(header) = line.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => {
                    status.head_oid = Some(commit_id(value, "status")?);
                }
                "branch.head" if value != "(detached)" => {
                    let name = BranchName::try_from(value.to_owned())
                        .map_err(|error| unparsable("status", &error.to_string()))?;
                    status.head = Head::Branch(name);
                }
                "branch.upstream" => status.upstream_name = Some(value.to_owned()),
                "branch.ab" => (status.ahead, status.behind) = parse_ahead_behind(value)?,
                _ => {}
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
            status.tracked_changes = status.tracked_changes.saturating_add(1);
        } else if line.starts_with("? ") {
            status.untracked = status.untracked.saturating_add(1);
        }
    }
    Ok(status)
}

fn parse_ahead_behind(value: &str) -> Result<(u32, u32), GitError> {
    let mut parts = value.split_whitespace();
    let ahead = parts
        .next()
        .and_then(|p| p.strip_prefix('+'))
        .and_then(|n| n.parse().ok());
    let behind = parts
        .next()
        .and_then(|p| p.strip_prefix('-'))
        .and_then(|n| n.parse().ok());
    ahead
        .zip(behind)
        .ok_or_else(|| unparsable("status", &format!("branch.ab {value:?}")))
}

/// Marker files Git leaves while an operation is unfinished.
async fn operation_in_progress(git_dir: &Path) -> Option<GitOperation> {
    let markers = [
        ("rebase-merge", GitOperation::Rebase),
        ("rebase-apply", GitOperation::Rebase),
        ("MERGE_HEAD", GitOperation::Merge),
        ("CHERRY_PICK_HEAD", GitOperation::CherryPick),
        ("REVERT_HEAD", GitOperation::Revert),
        ("BISECT_LOG", GitOperation::Bisect),
    ];
    for (marker, operation) in markers {
        if tokio::fs::try_exists(git_dir.join(marker))
            .await
            .unwrap_or(false)
        {
            return Some(operation);
        }
    }
    None
}

fn commit_id(value: &str, action: &'static str) -> Result<CommitId, GitError> {
    CommitId::try_from(value).map_err(|error| unparsable(action, &error.to_string()))
}

fn unparsable(action: &'static str, message: &str) -> GitError {
    GitError::Unparsable {
        action,
        message: message.to_owned(),
    }
}

fn path_str<'a>(path: &'a Path, action: &'static str) -> Result<&'a str, GitError> {
    path.to_str()
        .ok_or_else(|| unparsable(action, "repository path is not valid UTF-8"))
}

fn last_line(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OID: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn parses_branch_headers_and_counts() {
        let stdout = format!(
            "# branch.oid {OID}\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +1 -4\n\
             1 .M N... 100644 100644 100644 a b src/lib.rs\n\
             2 R. N... 100644 100644 100644 a b R100 new.rs\told.rs\n\
             u UU N... 100644 100644 100644 100644 a b c conflict.rs\n\
             ? notes.txt\n? scratch/\n! target/\n"
        );
        let status = parse_status(&stdout).expect("BUG: valid output");
        assert_eq!(
            status.head,
            Head::Branch(BranchName::try_from("main".to_owned()).expect("BUG"))
        );
        assert_eq!(status.head_oid.expect("BUG").as_str(), OID);
        assert_eq!(status.upstream_name.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (1, 4));
        assert_eq!(status.tracked_changes, 3);
        assert_eq!(status.untracked, 2);
    }

    #[test]
    fn parses_detached_head() {
        let status =
            parse_status(&format!("# branch.oid {OID}\n# branch.head (detached)\n")).expect("BUG");
        assert_eq!(status.head, Head::Detached);
        assert!(status.upstream_name.is_none());
    }

    #[test]
    fn parses_branch_without_commits() {
        let status = parse_status("# branch.oid (initial)\n# branch.head main\n").expect("BUG");
        assert!(status.head_oid.is_none());
    }

    #[test]
    fn rejects_malformed_ahead_behind() {
        let error = parse_status(&format!("# branch.oid {OID}\n# branch.ab 1 4\n")).unwrap_err();
        assert!(matches!(error, GitError::Unparsable { .. }), "{error}");
    }

    #[test]
    fn last_line_skips_trailing_blank_lines() {
        assert_eq!(last_line("hint: x\nerror: boom\n\n"), "error: boom");
    }
}
