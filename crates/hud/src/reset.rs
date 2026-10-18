//! `--reset-config` and `--reset-data`: delete hud's files after confirmation.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::cli::Reset;

/// Files the reset deletes, in the order they are shown.
pub fn targets(reset: Reset, config: &Path, database: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(4);
    if reset.config {
        paths.push(config.to_path_buf());
    }
    if reset.data {
        // SQLite in WAL mode keeps recent writes in these side files; leaving
        // them behind would resurrect data in the next database.
        paths.push(database.to_path_buf());
        for suffix in ["-wal", "-shm"] {
            let mut side = database.as_os_str().to_owned();
            side.push(suffix);
            paths.push(PathBuf::from(side));
        }
    }
    paths
}

/// Said before deleting the database: telemetry only arrives while hud runs.
pub const DATA_WARNING: &str =
    "Received telemetry cannot be restored; AI usage is read again from the logs.";

/// Lists `paths`, adds `warning` if given, and asks; only `y` or `yes`
/// counts as consent.
///
/// # Errors
///
/// Fails when the terminal cannot be written or read.
pub fn confirm(
    paths: &[PathBuf],
    warning: Option<&str>,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<bool> {
    writeln!(output, "hud will delete:")?;
    for path in paths {
        writeln!(output, "  {}", path.display())?;
    }
    if let Some(warning) = warning {
        writeln!(output, "{warning}")?;
    }
    write!(output, "Continue? [y/N] ")?;
    output.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// What happened to one file.
#[derive(Debug, PartialEq, Eq)]
pub enum Removal {
    /// Deleted.
    Deleted,
    /// Was not there; not an error.
    Missing,
}

/// Deletes every path; stops at the first real error.
///
/// # Errors
///
/// Fails when a file exists but cannot be deleted.
pub fn remove(paths: &[PathBuf]) -> io::Result<Vec<(PathBuf, Removal)>> {
    paths
        .iter()
        .map(|path| match std::fs::remove_file(path) {
            Ok(()) => Ok((path.clone(), Removal::Deleted)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok((path.clone(), Removal::Missing))
            }
            Err(error) => Err(io::Error::new(
                error.kind(),
                format!("could not delete {}: {error}", path.display()),
            )),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Reset = Reset {
        config: true,
        data: true,
        yes: false,
    };

    #[test]
    fn data_reset_includes_the_sqlite_side_files() {
        let paths = targets(
            Reset {
                data: true,
                ..Reset::default()
            },
            Path::new("/c/config.toml"),
            Path::new("/d/hud.db"),
        );
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/d/hud.db"),
                PathBuf::from("/d/hud.db-wal"),
                PathBuf::from("/d/hud.db-shm"),
            ]
        );
    }

    #[test]
    fn config_reset_touches_only_the_config() {
        let paths = targets(
            Reset {
                config: true,
                ..Reset::default()
            },
            Path::new("/c/config.toml"),
            Path::new("/d/hud.db"),
        );
        assert_eq!(paths, vec![PathBuf::from("/c/config.toml")]);
    }

    #[test]
    fn deleting_the_database_warns_about_telemetry() {
        let mut shown = Vec::new();
        confirm(
            &[PathBuf::from("/d/hud.db")],
            Some(DATA_WARNING),
            "n\n".as_bytes(),
            &mut shown,
        )
        .expect("BUG: in memory");
        assert!(String::from_utf8_lossy(&shown).contains("telemetry cannot be restored"));
    }

    #[test]
    fn only_yes_confirms() {
        let paths = [PathBuf::from("/c/config.toml")];
        for (answer, expected) in [
            ("y\n", true),
            ("YES\n", true),
            ("\n", false),
            ("no\n", false),
            ("", false),
        ] {
            let mut shown = Vec::new();
            let confirmed =
                confirm(&paths, None, answer.as_bytes(), &mut shown).expect("BUG: in memory");
            assert_eq!(confirmed, expected, "answer {answer:?}");
            assert!(String::from_utf8_lossy(&shown).contains("/c/config.toml"));
        }
    }

    #[test]
    fn remove_deletes_existing_files_and_reports_missing_ones() {
        let dir = tempfile::tempdir().expect("BUG: temp dir");
        let config = dir.path().join("config.toml");
        let database = dir.path().join("hud.db");
        std::fs::write(&config, "x").expect("BUG: write");
        std::fs::write(&database, "x").expect("BUG: write");

        let results = remove(&targets(ALL, &config, &database)).expect("BUG: removable");
        assert_eq!(results[0], (config.clone(), Removal::Deleted));
        assert_eq!(results[1], (database.clone(), Removal::Deleted));
        assert_eq!(results[2].1, Removal::Missing);
        assert!(!config.exists() && !database.exists());
    }
}
