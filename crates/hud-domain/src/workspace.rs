//! Workspaces: configured folders under which repositories are discovered.

use std::fmt;
use std::path::{Path, PathBuf};

/// Validation errors of domain values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// A workspace path must be absolute so it does not depend on the working directory.
    #[error("workspace path must be absolute: {0}")]
    RelativeWorkspacePath(PathBuf),
    /// A repository path must be absolute.
    #[error("repository path must be absolute: {0}")]
    RelativeRepoPath(PathBuf),
    /// Branch names are non-empty and contain no whitespace.
    #[error("invalid branch name: {0:?}")]
    InvalidBranchName(String),
    /// Commit ids are 40 or 64 hexadecimal digits.
    #[error("invalid commit id: {0:?}")]
    InvalidCommitId(String),
    /// Package names are non-empty and contain no whitespace.
    #[error("invalid package name: {0:?}")]
    InvalidPackageName(String),
    /// Model ids are non-empty and contain no whitespace.
    #[error("invalid model id: {0:?}")]
    InvalidModelId(String),
}

/// Absolute path of a workspace folder. Existence is not required: a missing
/// folder (e.g. an unmounted volume) is a state, not an invalid value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspacePath(PathBuf);

impl WorkspacePath {
    /// The path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl TryFrom<PathBuf> for WorkspacePath {
    type Error = DomainError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        if path.is_absolute() {
            Ok(Self(path))
        } else {
            Err(DomainError::RelativeWorkspacePath(path))
        }
    }
}

impl fmt::Display for WorkspacePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_path_is_accepted() {
        let path = WorkspacePath::try_from(PathBuf::from("/Users/me/projects"));
        assert!(path.is_ok());
    }

    #[test]
    fn relative_path_is_rejected() {
        let error = WorkspacePath::try_from(PathBuf::from("projects")).unwrap_err();
        assert_eq!(
            error,
            DomainError::RelativeWorkspacePath(PathBuf::from("projects"))
        );
    }
}
