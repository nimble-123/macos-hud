//! `GitCli` against real repositories: a bare upstream, a working clone and a
//! second clone that pushes new upstream commits.
#![expect(
    clippy::expect_used,
    reason = "fixture setup must fail loudly when the environment is broken"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use hud_application::ports::git::{FfOutcome, GitPort};
use hud_domain::git::{GitOperation, RepoPath, SkipReason, SyncDecision, SyncMode, sync_decision};
use hud_infrastructure::git::GitCli;
use hud_infrastructure::process::TokioProcessRunner;
use tempfile::TempDir;

const AUTO: SyncMode = SyncMode::Auto { opted_out: false };

struct Fixture {
    _dir: TempDir,
    work: PathBuf,
    other: PathBuf,
}

/// Runs git for test setup, isolated from the developer's global config.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("BUG: git must be installed for integration tests");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commit(dir: &Path, file: &str, content: &str) {
    std::fs::write(dir.join(file), content).expect("BUG: write test file");
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", &format!("change {file}")]);
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("BUG: temp dir");
        let upstream = dir.path().join("upstream.git");
        git(
            dir.path(),
            &[
                "init",
                "-q",
                "--bare",
                "--initial-branch=main",
                upstream.to_str().expect("BUG"),
            ],
        );
        let work = dir.path().join("work");
        let other = dir.path().join("other");
        git(
            dir.path(),
            &["clone", "-q", upstream.to_str().expect("BUG"), "work"],
        );
        git(&work, &["checkout", "-q", "-b", "main"]);
        commit(&work, "README.md", "hello\n");
        git(&work, &["push", "-q", "-u", "origin", "main"]);
        git(
            dir.path(),
            &["clone", "-q", upstream.to_str().expect("BUG"), "other"],
        );
        Self {
            _dir: dir,
            work,
            other,
        }
    }

    /// Pushes `n` new commits to the upstream from the second clone.
    fn push_upstream(&self, n: u32) {
        for i in 0..n {
            commit(&self.other, &format!("remote-{i}.txt"), "remote\n");
        }
        git(&self.other, &["push", "-q"]);
    }

    fn repo(&self) -> RepoPath {
        RepoPath::try_from(self.work.clone()).expect("BUG: absolute path")
    }

    fn cli() -> GitCli<TokioProcessRunner> {
        GitCli::new(
            TokioProcessRunner::new(std::env::var_os("PATH").unwrap_or_default()),
            "git",
        )
    }
}

#[tokio::test]
async fn clean_branch_behind_upstream_is_fast_forwarded() {
    let fixture = Fixture::new();
    fixture.push_upstream(3);
    let cli = Fixture::cli();

    cli.fetch(&fixture.repo()).await.expect("BUG: fetch");
    let status = cli.status(&fixture.repo()).await.expect("BUG: status");
    assert_eq!((status.ahead, status.behind), (0, 3));

    let SyncDecision::FastForward(permit) = sync_decision(&fixture.repo(), &status, AUTO) else {
        panic!("expected a permit, got {status:?}");
    };
    let outcome = cli.fast_forward(permit).await.expect("BUG: fast-forward");
    assert!(
        matches!(outcome, FfOutcome::Advanced { commits: 3, .. }),
        "{outcome:?}"
    );

    let after = cli.status(&fixture.repo()).await.expect("BUG: status");
    assert_eq!((after.ahead, after.behind), (0, 0));
    assert_eq!(
        git(&fixture.work, &["rev-parse", "HEAD"]),
        git(&fixture.other, &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn untracked_file_does_not_block_and_survives() {
    let fixture = Fixture::new();
    fixture.push_upstream(1);
    std::fs::write(fixture.work.join("notes.txt"), "mine\n").expect("BUG: write");
    let cli = Fixture::cli();
    cli.fetch(&fixture.repo()).await.expect("BUG: fetch");
    let status = cli.status(&fixture.repo()).await.expect("BUG: status");
    assert_eq!(status.untracked, 1);

    let SyncDecision::FastForward(permit) = sync_decision(&fixture.repo(), &status, AUTO) else {
        panic!("expected a permit");
    };
    let _ = cli.fast_forward(permit).await.expect("BUG: fast-forward");
    assert_eq!(
        std::fs::read_to_string(fixture.work.join("notes.txt")).expect("BUG"),
        "mine\n"
    );
}

#[tokio::test]
async fn tracked_change_is_skipped() {
    let fixture = Fixture::new();
    fixture.push_upstream(1);
    std::fs::write(fixture.work.join("README.md"), "edited\n").expect("BUG: write");
    let cli = Fixture::cli();
    cli.fetch(&fixture.repo()).await.expect("BUG: fetch");
    let status = cli.status(&fixture.repo()).await.expect("BUG: status");
    assert_eq!(
        sync_decision(&fixture.repo(), &status, AUTO),
        SyncDecision::Skip(SkipReason::TrackedChanges)
    );
}

#[tokio::test]
async fn diverged_branch_is_skipped() {
    let fixture = Fixture::new();
    fixture.push_upstream(2);
    commit(&fixture.work, "local.txt", "local\n");
    let cli = Fixture::cli();
    cli.fetch(&fixture.repo()).await.expect("BUG: fetch");
    let status = cli.status(&fixture.repo()).await.expect("BUG: status");
    assert_eq!(
        sync_decision(&fixture.repo(), &status, AUTO),
        SyncDecision::Skip(SkipReason::Diverged {
            ahead: 1,
            behind: 2
        })
    );
}

#[tokio::test]
async fn detached_head_is_skipped() {
    let fixture = Fixture::new();
    git(&fixture.work, &["checkout", "-q", "--detach"]);
    let status = Fixture::cli()
        .status(&fixture.repo())
        .await
        .expect("BUG: status");
    assert_eq!(
        sync_decision(&fixture.repo(), &status, AUTO),
        SyncDecision::Skip(SkipReason::DetachedHead)
    );
}

#[tokio::test]
async fn branch_without_upstream_is_skipped() {
    let fixture = Fixture::new();
    git(&fixture.work, &["checkout", "-q", "-b", "feature"]);
    let status = Fixture::cli()
        .status(&fixture.repo())
        .await
        .expect("BUG: status");
    assert!(status.upstream.is_none());
    assert_eq!(
        sync_decision(&fixture.repo(), &status, AUTO),
        SyncDecision::Skip(SkipReason::NoUpstream)
    );
}

#[tokio::test]
async fn unfinished_rebase_is_detected() {
    let fixture = Fixture::new();
    commit(&fixture.other, "README.md", "remote edit\n");
    git(&fixture.other, &["push", "-q"]);
    commit(&fixture.work, "README.md", "local edit\n");
    git(&fixture.work, &["fetch", "-q"]);
    git(&fixture.work, &["rebase", "origin/main"]); // stops with a conflict
    let status = Fixture::cli()
        .status(&fixture.repo())
        .await
        .expect("BUG: status");
    assert_eq!(status.operation, Some(GitOperation::Rebase));
}

#[tokio::test]
async fn moved_head_returns_state_changed_and_changes_nothing() {
    let fixture = Fixture::new();
    fixture.push_upstream(1);
    let cli = Fixture::cli();
    cli.fetch(&fixture.repo()).await.expect("BUG: fetch");
    let status = cli.status(&fixture.repo()).await.expect("BUG: status");
    let SyncDecision::FastForward(permit) = sync_decision(&fixture.repo(), &status, AUTO) else {
        panic!("expected a permit");
    };

    commit(&fixture.work, "late.txt", "committed after the decision\n");
    let head_before = git(&fixture.work, &["rev-parse", "HEAD"]);
    let outcome = cli
        .fast_forward(permit)
        .await
        .expect("BUG: fast-forward call");
    assert_eq!(outcome, FfOutcome::StateChanged);
    assert_eq!(git(&fixture.work, &["rev-parse", "HEAD"]), head_before);
}
