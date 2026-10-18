//! [`RepoDiscovery`] adapter that walks the file system.

use std::path::Path;

use hud_application::ports::discovery::{DiscoveryError, RepoDiscovery};
use hud_application::settings::ScanSettings;
use hud_domain::WorkspacePath;
use hud_domain::git::RepoPath;

/// Finds Git repositories by looking for `.git` (folder or worktree file).
///
/// Descends into repositories as well: a folder inside one with its own `.git`
/// folder is an independent clone. A `.git` file there marks a submodule or
/// worktree that belongs to the outer repository; it is neither listed nor
/// entered. Hidden folders, excluded names and symlinks are never entered.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsDiscovery;

impl RepoDiscovery for FsDiscovery {
    async fn discover(
        &self,
        workspace: &WorkspacePath,
        scan: &ScanSettings,
    ) -> Result<Vec<RepoPath>, DiscoveryError> {
        let root = workspace.as_path().to_path_buf();
        let scan = scan.clone();
        tokio::task::spawn_blocking(move || walk(&root, &scan))
            .await
            .map_err(|error| DiscoveryError::Unreadable {
                path: workspace.to_string(),
                message: error.to_string(),
            })?
    }
}

fn walk(root: &Path, scan: &ScanSettings) -> Result<Vec<RepoPath>, DiscoveryError> {
    if !root.is_dir() {
        return Err(DiscoveryError::Unavailable(root.display().to_string()));
    }
    let mut found = Vec::new();
    // Each folder carries whether it lies inside a repository found above it.
    let mut pending = vec![(root.to_path_buf(), 0u8, false)];
    while let Some((dir, depth, nested)) = pending.pop() {
        let mut inside = nested;
        match std::fs::metadata(dir.join(".git")) {
            Ok(git) if nested && !git.is_dir() => continue,
            Ok(_) => {
                if let Ok(repo) = RepoPath::try_from(dir.clone()) {
                    found.push(repo);
                }
                inside = true;
            }
            Err(_) => {}
        }
        if depth >= scan.max_depth {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if dir == root => {
                return Err(DiscoveryError::Unreadable {
                    path: root.display().to_string(),
                    message: error.to_string(),
                });
            }
            // A single unreadable sub-folder must not hide the rest.
            Err(error) => {
                tracing::debug!(dir = %dir.display(), %error, "skipping unreadable folder");
                continue;
            }
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_dir() {
                continue; // also skips symlinks, which report as symlink, not dir
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || scan.exclude.iter().any(|excluded| *excluded == name) {
                continue;
            }
            pending.push((entry.path(), depth + 1, inside));
        }
    }
    found.sort();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkrepo(path: &Path) {
        std::fs::create_dir_all(path.join(".git")).expect("BUG: create repo");
    }

    fn names(root: &Path, repos: &[RepoPath]) -> Vec<String> {
        repos
            .iter()
            .map(|r| {
                r.as_path()
                    .strip_prefix(root)
                    .expect("BUG")
                    .display()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn finds_repositories_within_max_depth() {
        let dir = tempfile::tempdir().expect("BUG");
        mkrepo(&dir.path().join("api"));
        mkrepo(&dir.path().join("group/web"));
        mkrepo(&dir.path().join("a/b/c/too-deep"));
        let scan = ScanSettings {
            max_depth: 2,
            exclude: vec![],
        };
        let repos = walk(dir.path(), &scan).expect("BUG");
        assert_eq!(names(dir.path(), &repos), vec!["api", "group/web"]);
    }

    #[test]
    fn finds_independent_clones_inside_a_repository() {
        let dir = tempfile::tempdir().expect("BUG");
        mkrepo(&dir.path().join("netzbau"));
        mkrepo(&dir.path().join("netzbau/sap/bau-sap-a"));
        mkrepo(&dir.path().join("netzbau/middleware/facade"));
        let repos = walk(dir.path(), &ScanSettings::default()).expect("BUG");
        assert_eq!(
            names(dir.path(), &repos),
            vec![
                "netzbau",
                "netzbau/middleware/facade",
                "netzbau/sap/bau-sap-a"
            ]
        );
    }

    #[test]
    fn skips_submodules_and_their_contents() {
        let dir = tempfile::tempdir().expect("BUG");
        mkrepo(&dir.path().join("api"));
        std::fs::create_dir_all(dir.path().join("api/vendor/lib")).expect("BUG");
        std::fs::write(
            dir.path().join("api/vendor/lib/.git"),
            "gitdir: ../../.git/modules/lib\n",
        )
        .expect("BUG");
        mkrepo(&dir.path().join("api/vendor/lib/inner"));
        let repos = walk(dir.path(), &ScanSettings::default()).expect("BUG");
        assert_eq!(names(dir.path(), &repos), vec!["api"]);
    }

    #[test]
    fn nested_repositories_count_towards_max_depth() {
        let dir = tempfile::tempdir().expect("BUG");
        mkrepo(&dir.path().join("outer"));
        mkrepo(&dir.path().join("outer/group/inner"));
        let scan = ScanSettings {
            max_depth: 2,
            exclude: vec![],
        };
        let repos = walk(dir.path(), &scan).expect("BUG");
        assert_eq!(names(dir.path(), &repos), vec!["outer"]);
    }

    #[test]
    fn skips_excluded_and_hidden_folders() {
        let dir = tempfile::tempdir().expect("BUG");
        mkrepo(&dir.path().join("node_modules/pkg"));
        mkrepo(&dir.path().join(".cache/repo"));
        mkrepo(&dir.path().join("app"));
        let repos = walk(dir.path(), &ScanSettings::default()).expect("BUG");
        assert_eq!(names(dir.path(), &repos), vec!["app"]);
    }

    #[test]
    fn recognises_worktrees_with_a_git_file() {
        let dir = tempfile::tempdir().expect("BUG");
        std::fs::create_dir_all(dir.path().join("wt")).expect("BUG");
        std::fs::write(dir.path().join("wt/.git"), "gitdir: /elsewhere\n").expect("BUG");
        let repos = walk(dir.path(), &ScanSettings::default()).expect("BUG");
        assert_eq!(names(dir.path(), &repos), vec!["wt"]);
    }

    #[test]
    fn does_not_follow_symlinks() {
        let dir = tempfile::tempdir().expect("BUG");
        let outside = tempfile::tempdir().expect("BUG");
        mkrepo(&outside.path().join("elsewhere"));
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).expect("BUG");
        let repos = walk(dir.path(), &ScanSettings::default()).expect("BUG");
        assert!(repos.is_empty(), "{repos:?}");
    }

    #[test]
    fn missing_workspace_is_unavailable() {
        let error = walk(
            Path::new("/Volumes/hud-not-mounted"),
            &ScanSettings::default(),
        )
        .unwrap_err();
        assert!(matches!(error, DiscoveryError::Unavailable(_)));
    }

    #[test]
    fn workspace_that_is_itself_a_repository_is_found() {
        let dir = tempfile::tempdir().expect("BUG");
        mkrepo(dir.path());
        let repos = walk(dir.path(), &ScanSettings::default()).expect("BUG");
        assert_eq!(repos.len(), 1);
    }
}
