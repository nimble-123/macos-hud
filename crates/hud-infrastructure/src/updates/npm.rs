//! Globally installed npm packages.

use std::collections::BTreeMap;
use std::time::Duration;

use futures::future::BoxFuture;
use hud_application::ports::updates::{
    Availability, InstallOutcome, SourceCheck, UpdateError, UpdateSource,
};
use hud_domain::update::{
    InstallPermit, InstallTarget, PackageKind, PackageName, SourceId, UpdateCandidate, Version,
};
use serde::Deserialize;

use crate::process::{ProcessError, ProcessOutput, ProcessRunner, ProcessSpec};

const TOOL: &str = "npm";
const QUICK: Duration = Duration::from_secs(30);
const OUTDATED: Duration = Duration::from_secs(120);
const INSTALL: Duration = Duration::from_mins(15);

/// npm through its CLI, from the PATH of the login shell (nvm and friends).
#[derive(Debug, Clone)]
pub struct NpmGlobal<R> {
    runner: R,
    program: String,
}

impl<R: ProcessRunner> NpmGlobal<R> {
    /// `program` is `npm` or an explicit path from the config.
    pub fn new(runner: R, program: impl Into<String>) -> Self {
        Self {
            runner,
            program: program.into(),
        }
    }

    async fn run(&self, args: &[&str], timeout: Duration) -> Result<ProcessOutput, UpdateError> {
        let mut spec = ProcessSpec::new(self.program.clone(), args, timeout);
        spec.env = [
            ("NO_COLOR", "1"),
            ("npm_config_fund", "false"),
            ("npm_config_update_notifier", "false"),
        ]
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
        self.runner.run(spec).await.map_err(|error| match error {
            ProcessError::NotFound { .. } => UpdateError::Missing(TOOL),
            ProcessError::TimedOut { .. } => UpdateError::TimedOut(TOOL),
            ProcessError::Io { source, .. } => UpdateError::Failed {
                tool: TOOL,
                message: source.to_string(),
            },
        })
    }

    async fn check_inner(&self) -> Result<SourceCheck, UpdateError> {
        let output = self
            .run(&["outdated", "--global", "--json"], OUTDATED)
            .await?;
        // npm exits with 1 when updates exist; only other codes are failures.
        match output.code {
            Some(0 | 1) => {}
            _ => {
                return Err(UpdateError::Failed {
                    tool: TOOL,
                    message: last_line(&output.stderr),
                });
            }
        }
        Ok(SourceCheck {
            candidates: parse_outdated(&output.stdout)?,
            installed: Vec::new(),
            updates_known: true,
        })
    }

    async fn install_inner(&self, permit: InstallPermit) -> Result<InstallOutcome, UpdateError> {
        let InstallTarget::Package(candidate) = permit.target() else {
            return Err(UpdateError::WrongTarget(TOOL));
        };
        if candidate.kind != PackageKind::NpmGlobal {
            return Err(UpdateError::WrongTarget(TOOL));
        }
        // Install exactly the version that was checked, not whatever is latest by now.
        let spec = format!("{}@{}", candidate.name, candidate.latest);
        let output = self.run(&["install", "--global", &spec], INSTALL).await?;
        if output.code == Some(0) {
            Ok(InstallOutcome::Installed)
        } else {
            Err(UpdateError::Failed {
                tool: TOOL,
                message: last_line(&output.stderr),
            })
        }
    }
}

impl<R: ProcessRunner> UpdateSource for NpmGlobal<R> {
    fn id(&self) -> SourceId {
        SourceId::Npm
    }

    fn availability(&self) -> BoxFuture<'_, Availability> {
        Box::pin(async move {
            match self.run(&["--version"], QUICK).await {
                Ok(output) if output.code == Some(0) => {
                    Availability::Available(output.stdout.trim().to_owned())
                }
                Ok(output) => Availability::Broken(last_line(&output.stderr)),
                Err(UpdateError::Missing(_)) => Availability::Missing,
                Err(error) => Availability::Broken(error.to_string()),
            }
        })
    }

    fn check(&self) -> BoxFuture<'_, Result<SourceCheck, UpdateError>> {
        Box::pin(self.check_inner())
    }

    fn install(&self, permit: InstallPermit) -> BoxFuture<'_, Result<InstallOutcome, UpdateError>> {
        Box::pin(self.install_inner(permit))
    }
}

#[derive(Debug, Deserialize)]
struct Entry {
    current: Option<String>,
    latest: Option<String>,
}

fn parse_outdated(stdout: &str) -> Result<Vec<UpdateCandidate>, UpdateError> {
    // No updates: npm prints nothing or `{}`.
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }
    let entries: BTreeMap<String, Entry> =
        serde_json::from_str(stdout).map_err(|error| UpdateError::Unparsable {
            tool: TOOL,
            message: error.to_string(),
        })?;
    entries
        .into_iter()
        // Entries without both versions (e.g. broken installs) cannot be updated sensibly.
        .filter_map(|(name, entry)| Some((name, entry.current?, entry.latest?)))
        .filter(|(_, current, latest)| current != latest)
        .map(|(name, current, latest)| {
            let name = PackageName::try_from(name).map_err(|error| UpdateError::Unparsable {
                tool: TOOL,
                message: error.to_string(),
            })?;
            Ok(UpdateCandidate {
                source: SourceId::Npm,
                kind: PackageKind::NpmGlobal,
                name,
                current: Version::new(current),
                latest: Version::new(latest),
                pinned: false,
            })
        })
        .collect()
}

fn last_line(text: &str) -> String {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use hud_domain::update::{
        AutoInstallPolicy, InstallDecision, InstallRequest, VersionBump, install_decision,
    };

    use super::*;
    use crate::updates::fake_runner::{FakeRunner, ProcessErrorKind};

    const FIXTURE: &str = include_str!("../../tests/fixtures/npm/outdated-global.json");

    fn permit(candidate: UpdateCandidate) -> InstallPermit {
        match install_decision(
            InstallTarget::Package(candidate),
            InstallRequest::Manual { confirmed: true },
            AutoInstallPolicy::default(),
            false,
        ) {
            InstallDecision::Allowed(permit) => permit,
            InstallDecision::Denied(reason) => panic!("BUG: {reason:?}"),
        }
    }

    #[test]
    fn parses_real_npm_output() {
        let candidates = parse_outdated(FIXTURE).expect("BUG");
        assert_eq!(candidates.len(), 4);
        let cds = candidates
            .iter()
            .find(|c| c.name.as_str() == "@sap/cds-dk")
            .expect("BUG");
        assert_eq!(
            (cds.current.as_str(), cds.latest.as_str()),
            ("9.7.2", "10.1.0")
        );
        assert_eq!(cds.bump(), VersionBump::Major);
    }

    #[test]
    fn empty_output_means_no_updates() {
        assert_eq!(parse_outdated("").expect("BUG"), Vec::new());
        assert_eq!(parse_outdated("{}").expect("BUG"), Vec::new());
    }

    #[test]
    fn entries_without_versions_are_skipped() {
        let json = r#"{"broken": {"wanted": "1.0.0", "location": "/x"}}"#;
        assert_eq!(parse_outdated(json).expect("BUG"), Vec::new());
    }

    #[tokio::test]
    async fn exit_code_one_with_updates_is_success() {
        let runner = FakeRunner::default().answer(&["outdated"], 1, FIXTURE, "");
        let check = NpmGlobal::new(runner, "npm").check().await.expect("BUG");
        assert_eq!(check.candidates.len(), 4);
    }

    #[tokio::test]
    async fn other_exit_codes_are_failures() {
        let runner =
            FakeRunner::default().answer(&["outdated"], 254, "", "npm error code ENOENT\n");
        let error = NpmGlobal::new(runner, "npm").check().await.unwrap_err();
        assert_eq!(
            error,
            UpdateError::Failed {
                tool: "npm",
                message: "npm error code ENOENT".into()
            }
        );
    }

    #[tokio::test]
    async fn missing_npm_is_reported_as_missing() {
        let runner = FakeRunner::default().fail(&["--version"], ProcessErrorKind::NotFound);
        assert_eq!(
            NpmGlobal::new(runner, "npm").availability().await,
            Availability::Missing
        );
    }

    #[tokio::test]
    async fn install_pins_the_checked_version() {
        let runner = FakeRunner::default().answer(&["install"], 0, "", "");
        let npm = NpmGlobal::new(runner, "npm");
        let candidate = parse_outdated(FIXTURE)
            .expect("BUG")
            .into_iter()
            .next()
            .expect("BUG");
        let expected = format!("{}@{}", candidate.name, candidate.latest);
        assert_eq!(
            npm.install(permit(candidate)).await,
            Ok(InstallOutcome::Installed)
        );
        assert_eq!(
            npm.runner.args_of_calls(),
            vec![vec!["install".to_owned(), "--global".to_owned(), expected]]
        );
    }

    #[tokio::test]
    async fn slow_install_is_reported_as_timed_out() {
        let runner = FakeRunner::default().fail(&["install"], ProcessErrorKind::TimedOut);
        let candidate = parse_outdated(FIXTURE)
            .expect("BUG")
            .into_iter()
            .next()
            .expect("BUG");
        let outcome = NpmGlobal::new(runner, "npm")
            .install(permit(candidate))
            .await;
        assert_eq!(outcome, Err(UpdateError::TimedOut("npm")));
    }
}
