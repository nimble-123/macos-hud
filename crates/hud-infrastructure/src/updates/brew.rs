//! Homebrew formulae and casks.

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

const TOOL: &str = "brew";
const QUICK: Duration = Duration::from_secs(30);
const INDEX_UPDATE: Duration = Duration::from_secs(120);
const OUTDATED: Duration = Duration::from_secs(120);
const UPGRADE: Duration = Duration::from_mins(15);

/// Homebrew through its CLI.
#[derive(Debug, Clone)]
pub struct Brew<R> {
    runner: R,
    program: String,
}

impl<R: ProcessRunner> Brew<R> {
    /// `program` is `brew` or an explicit path from the config.
    pub fn new(runner: R, program: impl Into<String>) -> Self {
        Self {
            runner,
            program: program.into(),
        }
    }

    async fn run(&self, args: &[&str], timeout: Duration) -> Result<ProcessOutput, UpdateError> {
        let mut spec = ProcessSpec::new(self.program.clone(), args, timeout);
        spec.env = [
            ("HOMEBREW_NO_AUTO_UPDATE", "1"),
            ("HOMEBREW_NO_ENV_HINTS", "1"),
            ("HOMEBREW_NO_COLOR", "1"),
            ("NONINTERACTIVE", "1"),
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
        // A failed index update (e.g. offline) still allows listing what is known locally.
        match self.run(&["update", "--quiet"], INDEX_UPDATE).await {
            Ok(output) if output.code == Some(0) => {}
            Ok(output) => tracing::warn!(stderr = %last_line(&output.stderr), "brew update failed"),
            Err(error) => tracing::warn!(%error, "brew update failed"),
        }
        let output = self.run(&["outdated", "--json=v2"], OUTDATED).await?;
        if output.code != Some(0) {
            return Err(UpdateError::Failed {
                tool: TOOL,
                message: last_line(&output.stderr),
            });
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
        let name = candidate.name.as_str();
        let args: Vec<&str> = match candidate.kind {
            PackageKind::Formula => vec!["upgrade", name],
            PackageKind::Cask => vec!["upgrade", "--cask", name],
            PackageKind::NpmGlobal | PackageKind::Skill => {
                return Err(UpdateError::WrongTarget(TOOL));
            }
        };
        let output = self.run(&args, UPGRADE).await?;
        if output.code == Some(0) {
            return Ok(InstallOutcome::Installed);
        }
        if needs_interaction(&output.stderr) || needs_interaction(&output.stdout) {
            return Ok(InstallOutcome::NeedsInteraction);
        }
        Err(UpdateError::Failed {
            tool: TOOL,
            message: last_line(&output.stderr),
        })
    }
}

impl<R: ProcessRunner> UpdateSource for Brew<R> {
    fn id(&self) -> SourceId {
        SourceId::Homebrew
    }

    fn availability(&self) -> BoxFuture<'_, Availability> {
        Box::pin(async move {
            match self.run(&["--version"], QUICK).await {
                Ok(output) if output.code == Some(0) => Availability::Available(
                    output
                        .stdout
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim_start_matches("Homebrew ")
                        .to_owned(),
                ),
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

/// `brew outdated --json=v2`; formulae and casks share one entry shape.
#[derive(Debug, Deserialize)]
struct Outdated {
    #[serde(default)]
    formulae: Vec<Entry>,
    #[serde(default)]
    casks: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    name: String,
    #[serde(default)]
    installed_versions: Vec<String>,
    current_version: String,
    #[serde(default)]
    pinned: bool,
}

fn parse_outdated(stdout: &str) -> Result<Vec<UpdateCandidate>, UpdateError> {
    let outdated: Outdated =
        serde_json::from_str(stdout).map_err(|error| UpdateError::Unparsable {
            tool: TOOL,
            message: error.to_string(),
        })?;
    let formulae = outdated
        .formulae
        .into_iter()
        .map(|entry| (PackageKind::Formula, entry));
    let casks = outdated
        .casks
        .into_iter()
        .map(|entry| (PackageKind::Cask, entry));
    formulae
        .chain(casks)
        .map(|(kind, entry)| {
            let name =
                PackageName::try_from(entry.name).map_err(|error| UpdateError::Unparsable {
                    tool: TOOL,
                    message: error.to_string(),
                })?;
            Ok(UpdateCandidate {
                source: SourceId::Homebrew,
                kind,
                name,
                // Several kegs can be installed; the newest is what gets replaced.
                current: Version::new(entry.installed_versions.last().cloned().unwrap_or_default()),
                latest: Version::new(entry.current_version),
                pinned: entry.pinned,
            })
        })
        .collect()
}

/// Casks that need administrator rights fail with closed stdin like this.
fn needs_interaction(output: &str) -> bool {
    let lower = output.to_lowercase();
    [
        "a terminal is required",
        "no tty present",
        "a password is required",
        "sudo: ",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
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

    const OUTDATED_JSON: &str = r#"{
  "formulae": [
    {"name": "wget", "installed_versions": ["1.21.3", "1.21.4"], "current_version": "1.24.5", "pinned": false, "pinned_version": null},
    {"name": "node@20", "installed_versions": ["20.11.0"], "current_version": "20.18.1", "pinned": true, "pinned_version": "20.11.0"}
  ],
  "casks": [
    {"name": "firefox", "installed_versions": ["118.0"], "current_version": "119.0", "pinned": false, "pinned_version": null}
  ]
}"#;

    fn permit_for(candidate: UpdateCandidate) -> InstallPermit {
        match install_decision(
            InstallTarget::Package(candidate),
            InstallRequest::Manual { confirmed: true },
            AutoInstallPolicy::default(),
            false,
        ) {
            InstallDecision::Allowed(permit) => permit,
            InstallDecision::Denied(reason) => panic!("BUG: denied {reason:?}"),
        }
    }

    fn cask(name: &str) -> UpdateCandidate {
        UpdateCandidate {
            source: SourceId::Homebrew,
            kind: PackageKind::Cask,
            name: PackageName::try_from(name.to_owned()).expect("BUG"),
            current: Version::new("118.0"),
            latest: Version::new("119.0"),
            pinned: false,
        }
    }

    #[test]
    fn parses_formulae_and_casks_from_the_v2_json() {
        let candidates = parse_outdated(OUTDATED_JSON).expect("BUG");
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].kind, PackageKind::Formula);
        assert_eq!(candidates[0].current.as_str(), "1.21.4");
        assert_eq!(candidates[0].latest.as_str(), "1.24.5");
        assert!(candidates[1].pinned);
        assert_eq!(candidates[2].kind, PackageKind::Cask);
    }

    #[test]
    fn empty_lists_mean_no_updates() {
        assert_eq!(
            parse_outdated("{\"formulae\": [], \"casks\": []}").expect("BUG"),
            Vec::new()
        );
    }

    #[tokio::test]
    async fn check_updates_the_index_then_lists_outdated_packages() {
        let runner = FakeRunner::default().answer(&["update"], 0, "", "").answer(
            &["outdated"],
            0,
            OUTDATED_JSON,
            "",
        );
        let brew = Brew::new(runner, "brew");
        let check = brew.check().await.expect("BUG");
        assert_eq!(check.candidates.len(), 3);
        assert!(check.updates_known);
        assert_eq!(
            brew.runner.args_of_calls(),
            vec![vec!["update", "--quiet"], vec!["outdated", "--json=v2"]]
        );
    }

    #[tokio::test]
    async fn failed_index_update_still_lists_known_updates() {
        let runner = FakeRunner::default()
            .answer(&["update"], 1, "", "Error: Failed to download")
            .answer(&["outdated"], 0, OUTDATED_JSON, "");
        assert_eq!(
            Brew::new(runner, "brew")
                .check()
                .await
                .expect("BUG")
                .candidates
                .len(),
            3
        );
    }

    #[tokio::test]
    async fn slow_outdated_is_reported_as_timed_out() {
        let runner = FakeRunner::default()
            .answer(&["update"], 0, "", "")
            .fail(&["outdated"], ProcessErrorKind::TimedOut);
        assert_eq!(
            Brew::new(runner, "brew").check().await,
            Err(UpdateError::TimedOut("brew"))
        );
    }

    #[tokio::test]
    async fn missing_brew_is_reported_as_missing() {
        let runner = FakeRunner::default().fail(&["--version"], ProcessErrorKind::NotFound);
        assert_eq!(
            Brew::new(runner, "brew").availability().await,
            Availability::Missing
        );
    }

    #[tokio::test]
    async fn availability_reports_the_version() {
        let runner = FakeRunner::default().answer(&["--version"], 0, "Homebrew 7.0.9\n", "");
        assert_eq!(
            Brew::new(runner, "brew").availability().await,
            Availability::Available("7.0.9".into())
        );
    }

    #[tokio::test]
    async fn cask_upgrade_uses_the_cask_flag_and_noninteractive_env() {
        let runner = FakeRunner::default().answer(&["upgrade"], 0, "", "");
        let brew = Brew::new(runner, "brew");
        assert_eq!(
            brew.install(permit_for(cask("firefox"))).await,
            Ok(InstallOutcome::Installed)
        );
        let calls = brew.runner.calls.lock().expect("BUG").clone();
        assert_eq!(calls[0].args, vec!["upgrade", "--cask", "firefox"]);
        assert!(
            calls[0]
                .env
                .contains(&("NONINTERACTIVE".to_owned(), "1".to_owned()))
        );
    }

    #[tokio::test]
    async fn password_prompt_is_reported_as_needs_interaction() {
        let runner = FakeRunner::default().answer(
            &["upgrade"],
            1,
            "",
            "sudo: a terminal is required to read the password\n",
        );
        let outcome = Brew::new(runner, "brew")
            .install(permit_for(cask("docker")))
            .await;
        assert_eq!(outcome, Ok(InstallOutcome::NeedsInteraction));
    }

    #[tokio::test]
    async fn other_failures_carry_the_last_error_line() {
        let runner = FakeRunner::default().answer(
            &["upgrade"],
            1,
            "",
            "==> Downloading\nError: checksum mismatch\n",
        );
        let outcome = Brew::new(runner, "brew")
            .install(permit_for(cask("x")))
            .await;
        assert_eq!(
            outcome,
            Err(UpdateError::Failed {
                tool: "brew",
                message: "Error: checksum mismatch".into()
            })
        );
    }

    #[tokio::test]
    async fn skills_permit_is_rejected() {
        let permit = match install_decision(
            InstallTarget::AllSkills,
            InstallRequest::Manual { confirmed: true },
            AutoInstallPolicy::default(),
            false,
        ) {
            InstallDecision::Allowed(permit) => permit,
            InstallDecision::Denied(_) => panic!("BUG"),
        };
        let outcome = Brew::new(FakeRunner::default(), "brew")
            .install(permit)
            .await;
        assert_eq!(outcome, Err(UpdateError::WrongTarget("brew")));
    }
}
