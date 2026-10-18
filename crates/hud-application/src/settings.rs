//! User settings as the application understands them, independent of the file format.

use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use hud_domain::WorkspacePath;

/// UI language preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguagePreference {
    /// Follow the system language.
    #[default]
    Auto,
    /// German.
    De,
    /// English.
    En,
}

/// Light/dark preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreference {
    /// Detect from the terminal background.
    #[default]
    Auto,
    /// Dark.
    Dark,
    /// Light.
    Light,
}

/// Glyph set preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconPreference {
    /// Unicode symbols.
    #[default]
    Unicode,
    /// Nerd Font icons.
    Nerd,
    /// Plain ASCII.
    Ascii,
}

/// How AI usage is paid for; decides how cost estimates are labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Billing {
    /// Flat subscription; costs are shown as API equivalent.
    #[default]
    Subscription,
    /// Pay per token.
    Api,
}

/// A configured workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSettings {
    /// Folder to scan.
    pub path: WorkspacePath,
    /// Whether clean repositories behind upstream are fast-forwarded automatically.
    pub auto_fast_forward: bool,
}

/// Repository discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSettings {
    /// Maximum folder depth below a workspace.
    pub max_depth: u8,
    /// Folder names that are never descended into.
    pub exclude: Vec<String>,
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self {
            max_depth: 3,
            exclude: vec!["node_modules".into(), ".build".into(), "target".into()],
        }
    }
}

/// Opt-ins for automatic installation, all off by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AutoInstallSettings {
    /// Homebrew formulae.
    pub brew_formulae: bool,
    /// Homebrew casks.
    pub brew_casks: bool,
    /// Global npm packages (never across a major version).
    pub npm: bool,
}

impl From<AutoInstallSettings> for hud_domain::update::AutoInstallPolicy {
    fn from(settings: AutoInstallSettings) -> Self {
        Self {
            brew_formulae: settings.brew_formulae,
            brew_casks: settings.brew_casks,
            npm: settings.npm,
        }
    }
}

/// Skills CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillsSettings {
    /// Whether the Skills source is shown at all.
    pub enabled: bool,
    /// Pinned version of the `skills` package run through `npx`.
    pub cli_version: Option<String>,
}

impl Default for SkillsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            cli_version: None,
        }
    }
}

/// One AI provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSettings {
    /// Whether usage is read.
    pub enabled: bool,
    /// How usage is paid for.
    pub billing: Billing,
}

/// The OTLP receiver for Claude Code telemetry (ADR 0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetrySettings {
    /// Whether hud listens at all.
    pub enabled: bool,
    /// Port on `127.0.0.1`.
    pub port: u16,
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 4318,
        }
    }
}

/// Explicit paths for external tools; `None` means look up in the login PATH.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolPaths {
    /// `git`.
    pub git: Option<std::path::PathBuf>,
    /// `brew`.
    pub brew: Option<std::path::PathBuf>,
    /// `npm`.
    pub npm: Option<std::path::PathBuf>,
}

/// Complete user settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// UI language.
    pub language: LanguagePreference,
    /// Light/dark.
    pub theme: ThemePreference,
    /// Glyph set.
    pub icons: IconPreference,
    /// Repository discovery.
    pub scan: ScanSettings,
    /// Configured workspaces; empty means the first-run setup is needed.
    pub workspaces: Vec<WorkspaceSettings>,
    /// How often repositories are fetched.
    pub fetch_interval: Duration,
    /// How often package updates are checked.
    pub update_check_interval: Duration,
    /// Automatic installation opt-ins.
    pub auto_install: AutoInstallSettings,
    /// Skills CLI.
    pub skills: SkillsSettings,
    /// Claude Code.
    pub claude: ProviderSettings,
    /// Codex.
    pub codex: ProviderSettings,
    /// OTLP receiver.
    pub telemetry: TelemetrySettings,
    /// Price overrides by model id, in micro-USD per million tokens (ADR 0009).
    pub prices: BTreeMap<String, hud_domain::usage::Price>,
    /// Explicit tool paths.
    pub tools: ToolPaths,
}

impl Settings {
    /// Whether the logs of `provider` are read.
    #[must_use]
    pub fn reads(&self, provider: hud_domain::usage::Provider) -> bool {
        match provider {
            hud_domain::usage::Provider::Claude => self.claude.enabled,
            hud_domain::usage::Provider::Codex => self.codex.enabled,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LanguagePreference::default(),
            theme: ThemePreference::default(),
            icons: IconPreference::default(),
            scan: ScanSettings::default(),
            workspaces: Vec::new(),
            fetch_interval: Duration::from_mins(15),
            update_check_interval: Duration::from_hours(6),
            auto_install: AutoInstallSettings::default(),
            skills: SkillsSettings::default(),
            claude: ProviderSettings {
                enabled: true,
                billing: Billing::Subscription,
            },
            codex: ProviderSettings {
                enabled: false,
                billing: Billing::Subscription,
            },
            telemetry: TelemetrySettings::default(),
            prices: BTreeMap::new(),
            tools: ToolPaths::default(),
        }
    }
}

impl Settings {
    /// True when no workspace is configured yet.
    #[must_use]
    pub fn needs_setup(&self) -> bool {
        self.workspaces.is_empty()
    }
}

/// Why settings could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The file exists but cannot be read.
    #[error("cannot read settings from {path}")]
    Read {
        /// File path.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not valid; the message includes line and column.
    #[error("invalid settings in {path}: {message}")]
    Invalid {
        /// File path.
        path: String,
        /// Parser or validation message.
        message: String,
    },
}

/// Choices made in the first-run setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialSetup {
    /// Workspace folders as entered; `~/` is allowed.
    pub workspaces: Vec<String>,
    /// Chosen glyph set.
    pub icons: IconPreference,
    /// Read Claude Code's logs.
    pub claude: bool,
    /// Read Codex's logs.
    pub codex: bool,
}

/// One change made in the settings tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsChange {
    /// UI language.
    Language(LanguagePreference),
    /// Light/dark.
    Theme(ThemePreference),
    /// Glyph set.
    Icons(IconPreference),
    /// Adds a workspace folder as entered; `~/` is allowed. Known folders are ignored.
    AddWorkspace(String),
    /// Removes a workspace.
    RemoveWorkspace(WorkspacePath),
    /// Turns automatic fast-forward of one workspace on or off.
    AutoFastForward {
        /// Workspace.
        workspace: WorkspacePath,
        /// New value.
        enabled: bool,
    },
    /// Turns one automatic installation opt-in on or off.
    AutoInstall {
        /// Which opt-in.
        kind: AutoInstallKind,
        /// New value.
        enabled: bool,
    },
    /// Turns reading one AI tool's logs on or off.
    AiSource {
        /// Which tool.
        provider: hud_domain::usage::Provider,
        /// New value.
        enabled: bool,
    },
}

/// The automatic installation opt-ins, one per kind of package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoInstallKind {
    /// Homebrew formulae.
    BrewFormulae,
    /// Homebrew casks.
    BrewCasks,
    /// Global npm packages.
    Npm,
}

/// Loads and updates user settings.
pub trait SettingsStore: Send + Sync {
    /// Loads the settings. A missing file yields [`Settings::default`].
    fn load(&self) -> impl Future<Output = Result<Settings, SettingsError>> + Send;

    /// Adds the setup choices to the file, keeping everything already in it,
    /// and returns the settings as they are afterwards.
    fn apply_setup(
        &self,
        setup: &InitialSetup,
    ) -> impl Future<Output = Result<Settings, SettingsError>> + Send;

    /// Applies one change to the file as it is now, keeping comments and
    /// everything else in it, and returns the settings afterwards.
    fn update(
        &self,
        change: &SettingsChange,
    ) -> impl Future<Output = Result<Settings, SettingsError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_documented_values() {
        let settings = Settings::default();
        assert_eq!(settings.fetch_interval, Duration::from_mins(15));
        assert_eq!(settings.update_check_interval, Duration::from_secs(21_600));
        assert_eq!(settings.auto_install, AutoInstallSettings::default());
        assert!(settings.claude.enabled);
        assert!(!settings.codex.enabled);
    }

    #[test]
    fn empty_workspaces_need_setup() {
        assert!(Settings::default().needs_setup());
    }
}
