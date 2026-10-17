//! Package updates and the rule for installing them.

use std::fmt;

use crate::DomainError;
use crate::action::Trigger;

/// Where an update comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceId {
    /// Homebrew formulae and casks.
    Homebrew,
    /// Global npm packages.
    Npm,
    /// Skills installed through the Skills CLI.
    Skills,
}

/// What kind of package an update is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageKind {
    /// Homebrew formula.
    Formula,
    /// Homebrew cask.
    Cask,
    /// Global npm package.
    NpmGlobal,
    /// Skill.
    Skill,
}

/// Name of a package as its package manager spells it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageName(String);

impl PackageName {
    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PackageName {
    type Error = DomainError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            Err(DomainError::InvalidPackageName(name))
        } else {
            Ok(Self(name))
        }
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A version as reported by the package manager. Not every package uses
/// semantic versioning (casks often do not), so the raw text is kept.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(String);

impl Version {
    /// Wraps the raw version text.
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The raw text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Lenient semantic version: leading `v` dropped, missing parts filled
    /// with zero, anything after the numeric prefix ignored (`119.0,abc`).
    fn semantic(&self) -> Option<semver::Version> {
        let text = self.0.trim().trim_start_matches('v');
        let numeric: String = text
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let mut parts = numeric.split('.').filter(|part| !part.is_empty());
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().map_or(Some(0), |part| part.parse().ok())?;
        let patch = parts.next().map_or(Some(0), |part| part.parse().ok())?;
        Some(semver::Version::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How big a step an update is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionBump {
    /// First number changes; may break things.
    Major,
    /// Second number changes.
    Minor,
    /// Anything smaller.
    Patch,
    /// Versions are not comparable.
    Unknown,
}

/// The size of the step from `current` to `latest`.
#[must_use]
pub fn version_bump(current: &Version, latest: &Version) -> VersionBump {
    match (current.semantic(), latest.semantic()) {
        (Some(from), Some(to)) if to.major != from.major => VersionBump::Major,
        (Some(from), Some(to)) if to.minor != from.minor => VersionBump::Minor,
        (Some(_), Some(_)) => VersionBump::Patch,
        _ => VersionBump::Unknown,
    }
}

/// An available update for one package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    /// Source.
    pub source: SourceId,
    /// Kind of package.
    pub kind: PackageKind,
    /// Package.
    pub name: PackageName,
    /// Installed version.
    pub current: Version,
    /// Version that would be installed.
    pub latest: Version,
    /// Held back by the user (`brew pin`); the package manager refuses to update it.
    pub pinned: bool,
}

impl UpdateCandidate {
    /// Size of this update.
    #[must_use]
    pub fn bump(&self) -> VersionBump {
        version_bump(&self.current, &self.latest)
    }
}

/// What an installation acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallTarget {
    /// One package.
    Package(UpdateCandidate),
    /// Every installed skill; the Skills CLI cannot check without installing.
    AllSkills,
}

/// Who asks for the installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallRequest {
    /// The user, after seeing a confirmation dialog or not.
    Manual {
        /// The user confirmed the system-wide change.
        confirmed: bool,
    },
    /// The scheduler.
    Auto,
}

/// Automatic installation opt-ins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AutoInstallPolicy {
    /// Homebrew formulae.
    pub brew_formulae: bool,
    /// Homebrew casks.
    pub brew_casks: bool,
    /// Global npm packages.
    pub npm: bool,
}

/// Why an installation is not allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// The package is pinned.
    Pinned,
    /// A manual installation was not confirmed.
    NotConfirmed,
    /// The source never installs automatically (Skills).
    SourceHasNoAuto,
    /// Automatic installation is off for this kind of package.
    NotOptedIn,
    /// npm across a major version is never automatic.
    MajorBump,
    /// npm versions that cannot be compared are never automatic.
    UnknownVersion,
    /// This cask asked for a password before; it is not retried automatically.
    NeedsInteractionPreviously,
}

/// Proof that [`install_decision`] allowed exactly this installation.
///
/// Only this module can create one, so installing without a passed rule
/// check does not compile (ADR 0003).
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a permit that is not used means the installation never happens"]
pub struct InstallPermit {
    target: InstallTarget,
    trigger: Trigger,
}

impl InstallPermit {
    /// What to install.
    #[must_use]
    pub fn target(&self) -> &InstallTarget {
        &self.target
    }

    /// Manual or automatic.
    #[must_use]
    pub fn trigger(&self) -> Trigger {
        self.trigger
    }
}

/// Outcome of [`install_decision`].
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub enum InstallDecision {
    /// All rules passed.
    Allowed(InstallPermit),
    /// A rule failed.
    Denied(DenyReason),
}

/// Decides whether an installation may run (PRD §4.3, figure 3).
///
/// `needed_interaction_before` is true when this cask version already
/// failed because it asked for a password.
pub fn install_decision(
    target: InstallTarget,
    request: InstallRequest,
    policy: AutoInstallPolicy,
    needed_interaction_before: bool,
) -> InstallDecision {
    if let InstallTarget::Package(candidate) = &target
        && candidate.pinned
    {
        return InstallDecision::Denied(DenyReason::Pinned);
    }
    let trigger = match request {
        InstallRequest::Manual { confirmed: false } => {
            return InstallDecision::Denied(DenyReason::NotConfirmed);
        }
        InstallRequest::Manual { confirmed: true } => Trigger::Manual,
        InstallRequest::Auto => {
            if let Some(reason) = auto_denial(&target, policy, needed_interaction_before) {
                return InstallDecision::Denied(reason);
            }
            Trigger::Auto
        }
    };
    InstallDecision::Allowed(InstallPermit { target, trigger })
}

fn auto_denial(
    target: &InstallTarget,
    policy: AutoInstallPolicy,
    needed_interaction_before: bool,
) -> Option<DenyReason> {
    let candidate = match target {
        InstallTarget::AllSkills => return Some(DenyReason::SourceHasNoAuto),
        InstallTarget::Package(candidate) => candidate,
    };
    match candidate.kind {
        PackageKind::Skill => Some(DenyReason::SourceHasNoAuto),
        PackageKind::Formula if !policy.brew_formulae => Some(DenyReason::NotOptedIn),
        PackageKind::Cask if !policy.brew_casks => Some(DenyReason::NotOptedIn),
        PackageKind::Cask if needed_interaction_before => {
            Some(DenyReason::NeedsInteractionPreviously)
        }
        PackageKind::NpmGlobal if !policy.npm => Some(DenyReason::NotOptedIn),
        PackageKind::NpmGlobal => match candidate.bump() {
            VersionBump::Major => Some(DenyReason::MajorBump),
            VersionBump::Unknown => Some(DenyReason::UnknownVersion),
            VersionBump::Minor | VersionBump::Patch => None,
        },
        PackageKind::Formula | PackageKind::Cask => None,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const ALL_ON: AutoInstallPolicy = AutoInstallPolicy {
        brew_formulae: true,
        brew_casks: true,
        npm: true,
    };

    fn candidate(kind: PackageKind, current: &str, latest: &str) -> UpdateCandidate {
        let source = match kind {
            PackageKind::Formula | PackageKind::Cask => SourceId::Homebrew,
            PackageKind::NpmGlobal => SourceId::Npm,
            PackageKind::Skill => SourceId::Skills,
        };
        UpdateCandidate {
            source,
            kind,
            name: PackageName::try_from("pkg".to_owned()).expect("BUG"),
            current: Version::new(current),
            latest: Version::new(latest),
            pinned: false,
        }
    }

    fn auto(target: InstallTarget, policy: AutoInstallPolicy) -> InstallDecision {
        install_decision(target, InstallRequest::Auto, policy, false)
    }

    #[test]
    fn version_bump_compares_lenient_versions() {
        assert_eq!(
            version_bump(&Version::new("9.7.2"), &Version::new("10.1.0")),
            VersionBump::Major
        );
        assert_eq!(
            version_bump(&Version::new("1.21.0"), &Version::new("1.33.0")),
            VersionBump::Minor
        );
        assert_eq!(
            version_bump(&Version::new("4.0.49"), &Version::new("4.0.71")),
            VersionBump::Patch
        );
        assert_eq!(
            version_bump(&Version::new("118.0"), &Version::new("119.0,20261001")),
            VersionBump::Major
        );
        assert_eq!(
            version_bump(&Version::new("v1.2"), &Version::new("1.2.1")),
            VersionBump::Patch
        );
        assert_eq!(
            version_bump(&Version::new("latest"), &Version::new("1.0")),
            VersionBump::Unknown
        );
    }

    #[test]
    fn manual_installation_needs_confirmation() {
        let target = InstallTarget::Package(candidate(PackageKind::NpmGlobal, "9.0.0", "10.0.0"));
        let decision = install_decision(
            target,
            InstallRequest::Manual { confirmed: false },
            ALL_ON,
            false,
        );
        assert_eq!(decision, InstallDecision::Denied(DenyReason::NotConfirmed));
    }

    #[test]
    fn confirmed_manual_installation_allows_major_npm_updates() {
        let target = InstallTarget::Package(candidate(PackageKind::NpmGlobal, "9.0.0", "10.0.0"));
        let decision = install_decision(
            target,
            InstallRequest::Manual { confirmed: true },
            AutoInstallPolicy::default(),
            false,
        );
        let InstallDecision::Allowed(permit) = decision else {
            panic!("expected a permit")
        };
        assert_eq!(permit.trigger(), Trigger::Manual);
    }

    #[test]
    fn pinned_packages_are_never_installed() {
        let mut pinned = candidate(PackageKind::Formula, "1.0", "1.1");
        pinned.pinned = true;
        let decision = install_decision(
            InstallTarget::Package(pinned),
            InstallRequest::Manual { confirmed: true },
            ALL_ON,
            false,
        );
        assert_eq!(decision, InstallDecision::Denied(DenyReason::Pinned));
    }

    #[test]
    fn automatic_installation_needs_the_opt_in_for_its_kind() {
        let policy = AutoInstallPolicy {
            brew_formulae: true,
            ..AutoInstallPolicy::default()
        };
        assert!(matches!(
            auto(
                InstallTarget::Package(candidate(PackageKind::Formula, "1.0", "1.1")),
                policy
            ),
            InstallDecision::Allowed(_)
        ));
        assert_eq!(
            auto(
                InstallTarget::Package(candidate(PackageKind::Cask, "1.0", "1.1")),
                policy
            ),
            InstallDecision::Denied(DenyReason::NotOptedIn)
        );
        assert_eq!(
            auto(
                InstallTarget::Package(candidate(PackageKind::NpmGlobal, "1.0.0", "1.1.0")),
                policy
            ),
            InstallDecision::Denied(DenyReason::NotOptedIn)
        );
    }

    #[test]
    fn skills_are_never_installed_automatically() {
        assert_eq!(
            auto(InstallTarget::AllSkills, ALL_ON),
            InstallDecision::Denied(DenyReason::SourceHasNoAuto)
        );
    }

    #[test]
    fn npm_major_and_unknown_versions_stay_manual() {
        assert_eq!(
            auto(
                InstallTarget::Package(candidate(PackageKind::NpmGlobal, "9.7.2", "10.1.0")),
                ALL_ON
            ),
            InstallDecision::Denied(DenyReason::MajorBump)
        );
        assert_eq!(
            auto(
                InstallTarget::Package(candidate(PackageKind::NpmGlobal, "latest", "1.0.0")),
                ALL_ON
            ),
            InstallDecision::Denied(DenyReason::UnknownVersion)
        );
        assert!(matches!(
            auto(
                InstallTarget::Package(candidate(PackageKind::NpmGlobal, "4.0.49", "4.0.71")),
                ALL_ON
            ),
            InstallDecision::Allowed(_)
        ));
    }

    #[test]
    fn cask_that_asked_for_a_password_is_not_retried_automatically() {
        let target = InstallTarget::Package(candidate(PackageKind::Cask, "118.0", "119.0"));
        let decision = install_decision(target, InstallRequest::Auto, ALL_ON, true);
        assert_eq!(
            decision,
            InstallDecision::Denied(DenyReason::NeedsInteractionPreviously)
        );
    }

    #[test]
    fn package_name_rejects_empty_and_whitespace() {
        assert!(PackageName::try_from(String::new()).is_err());
        assert!(PackageName::try_from("a b".to_owned()).is_err());
        assert!(PackageName::try_from("@sap/cds-dk".to_owned()).is_ok());
    }

    fn any_request() -> impl Strategy<
        Value = (
            PackageKind,
            String,
            String,
            bool,
            InstallRequest,
            AutoInstallPolicy,
            bool,
        ),
    > {
        let kind = prop_oneof![
            Just(PackageKind::Formula),
            Just(PackageKind::Cask),
            Just(PackageKind::NpmGlobal),
            Just(PackageKind::Skill)
        ];
        let version = prop_oneof![
            Just("1.0.0".to_owned()),
            Just("1.2.0".to_owned()),
            Just("2.0.0".to_owned()),
            Just("latest".to_owned())
        ];
        let request = prop_oneof![
            Just(InstallRequest::Auto),
            Just(InstallRequest::Manual { confirmed: true }),
            Just(InstallRequest::Manual { confirmed: false })
        ];
        let policy = (any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
            |(brew_formulae, brew_casks, npm)| AutoInstallPolicy {
                brew_formulae,
                brew_casks,
                npm,
            },
        );
        (
            kind,
            version.clone(),
            version,
            any::<bool>(),
            request,
            policy,
            any::<bool>(),
        )
    }

    proptest! {
        #[test]
        fn automatic_npm_never_crosses_a_major_version((kind, from, to, pinned, request, policy, asked) in any_request()) {
            let mut c = candidate(kind, &from, &to);
            c.pinned = pinned;
            let major = c.bump() == VersionBump::Major;
            let decision = install_decision(InstallTarget::Package(c), request, policy, asked);
            if kind == PackageKind::NpmGlobal && request == InstallRequest::Auto && major {
                prop_assert!(!matches!(decision, InstallDecision::Allowed(_)));
            }
        }

        #[test]
        fn skills_and_unconfirmed_requests_never_get_a_permit((kind, from, to, pinned, request, policy, asked) in any_request()) {
            let mut c = candidate(kind, &from, &to);
            c.pinned = pinned;
            let decision = install_decision(InstallTarget::Package(c), request, policy, asked);
            let skill_auto = kind == PackageKind::Skill && request == InstallRequest::Auto;
            let unconfirmed = request == InstallRequest::Manual { confirmed: false };
            if skill_auto || unconfirmed || pinned {
                prop_assert!(!matches!(decision, InstallDecision::Allowed(_)));
            }
        }

        #[test]
        fn automatic_permits_require_the_opt_in((kind, from, to, pinned, _request, policy, asked) in any_request()) {
            let mut c = candidate(kind, &from, &to);
            c.pinned = pinned;
            if let InstallDecision::Allowed(permit) = install_decision(InstallTarget::Package(c), InstallRequest::Auto, policy, asked) {
                prop_assert_eq!(permit.trigger(), Trigger::Auto);
                let opted_in = match kind {
                    PackageKind::Formula => policy.brew_formulae,
                    PackageKind::Cask => policy.brew_casks,
                    PackageKind::NpmGlobal => policy.npm,
                    PackageKind::Skill => false,
                };
                prop_assert!(opted_in);
            }
        }
    }
}
