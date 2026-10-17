//! Skills installed with the Skills CLI (`npx skills`, vercel-labs).
//!
//! The CLI cannot check for updates without installing them (#45), so this
//! source lists installed skills and offers "update all".

use std::time::Duration;

use futures::future::BoxFuture;
use hud_application::ports::updates::{
    Availability, InstallOutcome, SourceCheck, UpdateError, UpdateSource,
};
use hud_domain::update::{InstallPermit, InstallTarget, SourceId};
use serde::Deserialize;

use crate::process::{ProcessError, ProcessOutput, ProcessRunner, ProcessSpec};

const TOOL: &str = "skills";
/// Pinned so that npx never runs unreviewed new code (ARCHITECTURE §8.12).
pub const DEFAULT_CLI_VERSION: &str = "1.7.2";
const QUICK: Duration = Duration::from_secs(60);
const UPDATE: Duration = Duration::from_mins(15);

/// The Skills CLI, run through `npx` with telemetry switched off.
#[derive(Debug, Clone)]
pub struct Skills<R> {
    runner: R,
    npx: String,
    package: String,
}

impl<R: ProcessRunner> Skills<R> {
    /// `npx` is `npx` or an explicit path; `version` overrides the pinned CLI version.
    pub fn new(runner: R, npx: impl Into<String>, version: Option<&str>) -> Self {
        Self {
            runner,
            npx: npx.into(),
            package: format!("skills@{}", version.unwrap_or(DEFAULT_CLI_VERSION)),
        }
    }

    async fn run(&self, args: &[&str], timeout: Duration) -> Result<ProcessOutput, UpdateError> {
        let mut all = vec!["--yes", self.package.as_str()];
        all.extend_from_slice(args);
        let mut spec = ProcessSpec::new(self.npx.clone(), &all, timeout);
        spec.env = [
            ("DISABLE_TELEMETRY", "1"),
            ("DO_NOT_TRACK", "1"),
            ("NO_COLOR", "1"),
            ("npm_config_update_notifier", "false"),
        ]
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
        self.runner.run(spec).await.map_err(|error| match error {
            ProcessError::NotFound { .. } => UpdateError::Missing("npx"),
            ProcessError::TimedOut { .. } => UpdateError::TimedOut(TOOL),
            ProcessError::Io { source, .. } => UpdateError::Failed {
                tool: TOOL,
                message: source.to_string(),
            },
        })
    }

    async fn check_inner(&self) -> Result<SourceCheck, UpdateError> {
        let output = self.run(&["ls", "--global", "--json"], QUICK).await?;
        if output.code != Some(0) {
            return Err(UpdateError::Failed {
                tool: TOOL,
                message: last_line(&output.stderr),
            });
        }
        Ok(SourceCheck {
            candidates: Vec::new(),
            installed: parse_list(&output.stdout)?,
            updates_known: false,
        })
    }

    async fn install_inner(&self, permit: InstallPermit) -> Result<InstallOutcome, UpdateError> {
        if *permit.target() != InstallTarget::AllSkills {
            return Err(UpdateError::WrongTarget(TOOL));
        }
        let output = self.run(&["update", "--global", "--yes"], UPDATE).await?;
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

impl<R: ProcessRunner> UpdateSource for Skills<R> {
    fn id(&self) -> SourceId {
        SourceId::Skills
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
struct Installed {
    name: String,
}

fn parse_list(stdout: &str) -> Result<Vec<String>, UpdateError> {
    let installed: Vec<Installed> =
        serde_json::from_str(stdout).map_err(|error| UpdateError::Unparsable {
            tool: TOOL,
            message: error.to_string(),
        })?;
    let mut names: Vec<String> = installed.into_iter().map(|skill| skill.name).collect();
    names.sort();
    names.dedup();
    Ok(names)
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
        AutoInstallPolicy, InstallDecision, InstallRequest, install_decision,
    };

    use super::*;
    use crate::updates::fake_runner::{FakeRunner, ProcessErrorKind};

    const FIXTURE: &str = include_str!("../../tests/fixtures/skills/ls-global.json");

    fn all_skills_permit() -> InstallPermit {
        match install_decision(
            InstallTarget::AllSkills,
            InstallRequest::Manual { confirmed: true },
            AutoInstallPolicy::default(),
            false,
        ) {
            InstallDecision::Allowed(permit) => permit,
            InstallDecision::Denied(reason) => panic!("BUG: {reason:?}"),
        }
    }

    #[test]
    fn lists_skill_names_from_real_output() {
        assert_eq!(
            parse_list(FIXTURE).expect("BUG"),
            vec!["brag", "canvas-design", "diagram-design"]
        );
    }

    #[tokio::test]
    async fn check_lists_installed_skills_and_says_updates_are_unknown() {
        let runner = FakeRunner::default().answer(&["--yes"], 0, FIXTURE, "");
        let skills = Skills::new(runner, "npx", None);
        let check = skills.check().await.expect("BUG");
        assert!(!check.updates_known);
        assert_eq!(check.candidates, Vec::new());
        assert_eq!(check.installed.len(), 3);
        let calls = skills.runner.calls.lock().expect("BUG").clone();
        assert_eq!(
            calls[0].args,
            vec!["--yes", "skills@1.7.2", "ls", "--global", "--json"]
        );
        assert!(
            calls[0]
                .env
                .contains(&("DISABLE_TELEMETRY".to_owned(), "1".to_owned()))
        );
        assert!(
            calls[0]
                .env
                .contains(&("DO_NOT_TRACK".to_owned(), "1".to_owned()))
        );
    }

    #[tokio::test]
    async fn configured_version_overrides_the_pin() {
        let runner = FakeRunner::default().answer(&["--yes"], 0, "[]", "");
        let skills = Skills::new(runner, "npx", Some("2.0.0"));
        let _ = skills.check().await.expect("BUG");
        assert_eq!(skills.runner.args_of_calls()[0][1], "skills@2.0.0");
    }

    #[tokio::test]
    async fn update_all_runs_the_global_update() {
        let runner = FakeRunner::default().answer(&["--yes"], 0, "", "");
        let skills = Skills::new(runner, "npx", None);
        assert_eq!(
            skills.install(all_skills_permit()).await,
            Ok(InstallOutcome::Installed)
        );
        assert_eq!(
            skills.runner.args_of_calls()[0],
            vec!["--yes", "skills@1.7.2", "update", "--global", "--yes"]
        );
    }

    #[tokio::test]
    async fn missing_npx_is_reported_as_missing() {
        let runner = FakeRunner::default().fail(&["--yes"], ProcessErrorKind::NotFound);
        assert_eq!(
            Skills::new(runner, "npx", None).availability().await,
            Availability::Missing
        );
    }

    #[tokio::test]
    async fn slow_update_is_reported_as_timed_out() {
        let runner = FakeRunner::default().fail(&["--yes"], ProcessErrorKind::TimedOut);
        let outcome = Skills::new(runner, "npx", None)
            .install(all_skills_permit())
            .await;
        assert_eq!(outcome, Err(UpdateError::TimedOut("skills")));
    }
}
