//! `config.toml` adapter for the [`SettingsStore`] port.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hud_application::settings::{
    AutoInstallSettings, Billing, IconPreference, InitialSetup, LanguagePreference, PriceOverride,
    ProviderSettings, ScanSettings, Settings, SettingsError, SettingsStore, SkillsSettings,
    ThemePreference, ToolPaths, WorkspaceSettings,
};
use hud_domain::WorkspacePath;
use serde::Deserialize;

/// Reads settings from a TOML file.
#[derive(Debug, Clone)]
pub struct TomlSettings {
    path: PathBuf,
    home: PathBuf,
}

impl TomlSettings {
    /// Settings at `path`; `~/` in paths inside the file expands to `home`.
    #[must_use]
    pub fn new(path: PathBuf, home: PathBuf) -> Self {
        Self { path, home }
    }

    /// `$XDG_CONFIG_HOME/hud/config.toml`, else `~/.config/hud/config.toml`.
    #[must_use]
    pub fn default_path(home: &Path, xdg_config_home: Option<&Path>) -> PathBuf {
        xdg_config_home
            .filter(|dir| dir.is_absolute())
            .map_or_else(|| home.join(".config"), Path::to_path_buf)
            .join("hud/config.toml")
    }

    /// File location.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SettingsStore for TomlSettings {
    async fn load(&self) -> Result<Settings, SettingsError> {
        let text = match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settings::default());
            }
            Err(source) => {
                return Err(SettingsError::Read {
                    path: self.path.display().to_string(),
                    source,
                });
            }
        };
        parse(&text, &self.home).map_err(|message| SettingsError::Invalid {
            path: self.path.display().to_string(),
            message,
        })
    }

    async fn apply_setup(&self, setup: &InitialSetup) -> Result<Settings, SettingsError> {
        let invalid = |message| SettingsError::Invalid {
            path: self.path.display().to_string(),
            message,
        };
        let existing = self.read_text().await?;
        let text = with_setup(existing.as_deref(), setup).map_err(invalid)?;
        // Validate before writing: a setup that produces an invalid file is rejected.
        let settings = parse(&text, &self.home).map_err(invalid)?;
        self.write_atomically(&text).await?;
        Ok(settings)
    }
}

impl TomlSettings {
    async fn read_text(&self) -> Result<Option<String>, SettingsError> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SettingsError::Read {
                path: self.path.display().to_string(),
                source,
            }),
        }
    }

    /// Writes next to the target and renames, so a crash never leaves a half-written file.
    async fn write_atomically(&self, text: &str) -> Result<(), SettingsError> {
        let io_error = |source| SettingsError::Read {
            path: self.path.display().to_string(),
            source,
        };
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(io_error)?;
        }
        let temporary = self.path.with_extension("toml.tmp");
        tokio::fs::write(&temporary, text).await.map_err(io_error)?;
        tokio::fs::rename(&temporary, &self.path)
            .await
            .map_err(io_error)
    }
}

const NEW_FILE_HEADER: &str = "\
# hud configuration. Every key is optional; see docs/ARCHITECTURE.md §8.7.
# Changes made in hud's settings tab are written back here and keep your comments.

";

/// Adds setup choices to `existing` (or a new file), keeping comments and other keys.
fn with_setup(existing: Option<&str>, setup: &InitialSetup) -> Result<String, String> {
    let mut document: toml_edit::DocumentMut = existing
        .unwrap_or_default()
        .parse()
        .map_err(|error: toml_edit::TomlError| error.to_string())?;
    document["icons"] = toml_edit::value(match setup.icons {
        IconPreference::Unicode => "unicode",
        IconPreference::Nerd => "nerd",
        IconPreference::Ascii => "ascii",
    });
    let workspaces = document
        .entry("workspaces")
        .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| "workspaces must be an array of tables ([[workspaces]])".to_owned())?;
    let known: Vec<String> = workspaces
        .iter()
        .filter_map(|table| {
            table
                .get("path")
                .and_then(toml_edit::Item::as_str)
                .map(str::to_owned)
        })
        .collect();
    for path in &setup.workspaces {
        if known.contains(path) {
            continue;
        }
        let mut table = toml_edit::Table::new();
        table["path"] = toml_edit::value(path.as_str());
        table["auto_fast_forward"] = toml_edit::value(true);
        workspaces.push(table);
    }
    // toml_edit attaches comments of an empty document to its end, so the
    // header of a new file is prepended to the rendered text instead.
    Ok(match existing {
        Some(_) => document.to_string(),
        None => format!("{NEW_FILE_HEADER}{document}"),
    })
}

/// Parses and validates the file content.
fn parse(text: &str, home: &Path) -> Result<Settings, String> {
    let file: File = toml::from_str(text).map_err(|error| error.to_string())?;
    file.into_settings(home)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct File {
    language: Language,
    theme: Theme,
    icons: Icons,
    scan: Scan,
    workspaces: Vec<Workspace>,
    git: Git,
    updates: Updates,
    ai: Ai,
    tools: Tools,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Language {
    #[default]
    Auto,
    De,
    En,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Theme {
    #[default]
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Icons {
    #[default]
    Unicode,
    Nerd,
    Ascii,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Scan {
    max_depth: u8,
    exclude: Vec<String>,
}

impl Default for Scan {
    fn default() -> Self {
        let defaults = ScanSettings::default();
        Self {
            max_depth: defaults.max_depth,
            exclude: defaults.exclude,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Workspace {
    path: String,
    #[serde(default = "yes")]
    auto_fast_forward: bool,
}

const fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Git {
    fetch_interval_min: u32,
}

impl Default for Git {
    fn default() -> Self {
        Self {
            fetch_interval_min: 15,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Updates {
    check_interval_min: u32,
    brew: Brew,
    npm: Npm,
    skills: Skills,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            check_interval_min: 360,
            brew: Brew::default(),
            npm: Npm::default(),
            skills: Skills::default(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Brew {
    auto_install_formulae: bool,
    auto_install_casks: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Npm {
    auto_install: bool,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Skills {
    enabled: bool,
    cli_version: Option<String>,
}

impl Default for Skills {
    fn default() -> Self {
        Self {
            enabled: true,
            cli_version: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Ai {
    claude: Provider,
    codex: CodexProvider,
    prices: BTreeMap<String, Price>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BillingKind {
    #[default]
    Subscription,
    Api,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Provider {
    enabled: bool,
    billing: BillingKind,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            enabled: true,
            billing: BillingKind::default(),
        }
    }
}

/// Same fields as [`Provider`], but disabled by default.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CodexProvider {
    enabled: bool,
    billing: BillingKind,
}

/// USD per million tokens as written by the user.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Price {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Tools {
    git: Option<String>,
    brew: Option<String>,
    npm: Option<String>,
}

impl File {
    fn into_settings(self, home: &Path) -> Result<Settings, String> {
        let workspaces = self
            .workspaces
            .into_iter()
            .map(|workspace| {
                let path = WorkspacePath::try_from(expand_home(&workspace.path, home))
                    .map_err(|error| error.to_string())?;
                Ok(WorkspaceSettings {
                    path,
                    auto_fast_forward: workspace.auto_fast_forward,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let prices = self
            .ai
            .prices
            .into_iter()
            .map(|(model, price)| price.to_micro_usd(&model).map(|price| (model, price)))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        Ok(Settings {
            language: match self.language {
                Language::Auto => LanguagePreference::Auto,
                Language::De => LanguagePreference::De,
                Language::En => LanguagePreference::En,
            },
            theme: match self.theme {
                Theme::Auto => ThemePreference::Auto,
                Theme::Dark => ThemePreference::Dark,
                Theme::Light => ThemePreference::Light,
            },
            icons: match self.icons {
                Icons::Unicode => IconPreference::Unicode,
                Icons::Nerd => IconPreference::Nerd,
                Icons::Ascii => IconPreference::Ascii,
            },
            scan: ScanSettings {
                max_depth: self.scan.max_depth,
                exclude: self.scan.exclude,
            },
            workspaces,
            fetch_interval: minutes(self.git.fetch_interval_min, "git.fetch_interval_min")?,
            update_check_interval: minutes(
                self.updates.check_interval_min,
                "updates.check_interval_min",
            )?,
            auto_install: AutoInstallSettings {
                brew_formulae: self.updates.brew.auto_install_formulae,
                brew_casks: self.updates.brew.auto_install_casks,
                npm: self.updates.npm.auto_install,
            },
            skills: SkillsSettings {
                enabled: self.updates.skills.enabled,
                cli_version: self.updates.skills.cli_version,
            },
            claude: provider(self.ai.claude.enabled, &self.ai.claude.billing),
            codex: provider(self.ai.codex.enabled, &self.ai.codex.billing),
            prices,
            tools: ToolPaths {
                git: self.tools.git.map(|p| expand_home(&p, home)),
                brew: self.tools.brew.map(|p| expand_home(&p, home)),
                npm: self.tools.npm.map(|p| expand_home(&p, home)),
            },
        })
    }
}

fn provider(enabled: bool, billing: &BillingKind) -> ProviderSettings {
    ProviderSettings {
        enabled,
        billing: match billing {
            BillingKind::Subscription => Billing::Subscription,
            BillingKind::Api => Billing::Api,
        },
    }
}

fn minutes(value: u32, key: &str) -> Result<Duration, String> {
    if value == 0 {
        return Err(format!("{key} must be at least 1"));
    }
    Ok(Duration::from_secs(u64::from(value) * 60))
}

impl Price {
    fn to_micro_usd(&self, model: &str) -> Result<PriceOverride, String> {
        let convert = |usd: f64, field: &str| -> Result<u64, String> {
            if !usd.is_finite() || !(0.0..=1_000_000.0).contains(&usd) {
                return Err(format!(
                    "ai.prices.\"{model}\".{field} must be between 0 and 1000000"
                ));
            }
            // Range checked above, so the rounded value fits into u64.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "value is finite, non-negative and at most 1e12 after scaling"
            )]
            let micro = (usd * 1_000_000.0).round() as u64;
            Ok(micro)
        };
        Ok(PriceOverride {
            input: convert(self.input, "input")?,
            output: convert(self.output, "output")?,
            cache_write: convert(self.cache_write, "cache_write")?,
            cache_read: convert(self.cache_read, "cache_read")?,
        })
    }
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if path == "~" => home.to_path_buf(),
        None => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/test";

    fn parse_ok(text: &str) -> Settings {
        parse(text, Path::new(HOME)).expect("BUG: test input must parse")
    }

    #[test]
    fn empty_file_yields_defaults() {
        assert_eq!(parse_ok(""), Settings::default());
    }

    #[test]
    fn documented_example_parses() {
        let settings = parse_ok(
            r#"
language = "de"
theme = "light"
icons = "nerd"

[scan]
max_depth = 2
exclude = ["node_modules"]

[[workspaces]]
path = "~/workspace/projects"

[[workspaces]]
path = "/Volumes/Ext/projects"
auto_fast_forward = false

[git]
fetch_interval_min = 5

[updates]
check_interval_min = 60

[updates.brew]
auto_install_formulae = true

[updates.npm]
auto_install = true

[updates.skills]
cli_version = "1.2.3"

[ai.claude]
billing = "api"

[ai.codex]
enabled = true

[ai.prices."claude-opus-5-5"]
input = 3.0
output = 15.0
cache_write = 3.75
cache_read = 0.3

[tools]
npm = "~/.nvm/versions/node/v24.12.0/bin/npm"
"#,
        );
        assert_eq!(settings.language, LanguagePreference::De);
        assert_eq!(settings.theme, ThemePreference::Light);
        assert_eq!(settings.icons, IconPreference::Nerd);
        assert_eq!(
            settings.workspaces[0].path.as_path(),
            Path::new("/Users/test/workspace/projects")
        );
        assert!(settings.workspaces[0].auto_fast_forward);
        assert!(!settings.workspaces[1].auto_fast_forward);
        assert_eq!(settings.fetch_interval, Duration::from_secs(300));
        assert!(settings.auto_install.brew_formulae);
        assert!(!settings.auto_install.brew_casks);
        assert_eq!(settings.claude.billing, Billing::Api);
        assert!(settings.codex.enabled);
        assert_eq!(settings.prices["claude-opus-5-5"].cache_write, 3_750_000);
        assert_eq!(
            settings.tools.npm.as_deref(),
            Some(Path::new("/Users/test/.nvm/versions/node/v24.12.0/bin/npm"))
        );
    }

    #[test]
    fn unknown_key_is_reported_with_its_line() {
        let error = parse("[git]\nfetch_interval = 5\n", Path::new(HOME)).unwrap_err();
        assert!(error.contains("fetch_interval"), "{error}");
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn relative_workspace_path_is_rejected() {
        let error = parse("[[workspaces]]\npath = \"projects\"\n", Path::new(HOME)).unwrap_err();
        assert!(error.contains("absolute"), "{error}");
    }

    #[test]
    fn zero_interval_is_rejected() {
        let error = parse("[git]\nfetch_interval_min = 0\n", Path::new(HOME)).unwrap_err();
        assert!(error.contains("at least 1"), "{error}");
    }

    #[test]
    fn negative_price_is_rejected() {
        let text =
            "[ai.prices.m]\ninput = -1.0\noutput = 0.0\ncache_write = 0.0\ncache_read = 0.0\n";
        assert!(parse(text, Path::new(HOME)).is_err());
    }

    #[test]
    fn default_path_respects_absolute_xdg_config_home() {
        let path = TomlSettings::default_path(Path::new(HOME), Some(Path::new("/xdg")));
        assert_eq!(path, Path::new("/xdg/hud/config.toml"));
    }

    #[test]
    fn default_path_ignores_relative_xdg_config_home() {
        let path = TomlSettings::default_path(Path::new(HOME), Some(Path::new("rel")));
        assert_eq!(path, Path::new("/Users/test/.config/hud/config.toml"));
    }

    fn setup(paths: &[&str], icons: IconPreference) -> InitialSetup {
        InitialSetup {
            workspaces: paths.iter().map(ToString::to_string).collect(),
            icons,
        }
    }

    #[tokio::test]
    async fn setup_creates_a_commented_file_that_loads_again() {
        let dir = tempfile::tempdir().expect("BUG");
        let store = TomlSettings::new(dir.path().join("hud/config.toml"), PathBuf::from(HOME));
        let applied = store
            .apply_setup(&setup(&["~/workspace/projects"], IconPreference::Nerd))
            .await
            .expect("BUG");
        let loaded = store.load().await.expect("BUG");
        assert_eq!(applied, loaded);
        assert_eq!(loaded.icons, IconPreference::Nerd);
        assert_eq!(
            loaded.workspaces[0].path.as_path(),
            Path::new("/Users/test/workspace/projects")
        );
        assert!(loaded.workspaces[0].auto_fast_forward);
        let text = std::fs::read_to_string(store.path()).expect("BUG");
        assert!(text.starts_with("# hud configuration"), "{text}");
        assert_eq!(text.matches("# hud configuration").count(), 1, "{text}");
    }

    #[test]
    fn setup_keeps_existing_keys_comments_and_workspaces() {
        let existing = "# my comment\nlanguage = \"de\"\n\n[[workspaces]]\npath = \"/a\"\nauto_fast_forward = false\n";
        let text = with_setup(
            Some(existing),
            &setup(&["/a", "/b"], IconPreference::Unicode),
        )
        .expect("BUG");
        assert!(text.contains("# my comment"), "{text}");
        assert!(text.contains("language = \"de\""), "{text}");
        let settings = parse(&text, Path::new(HOME)).expect("BUG");
        let paths: Vec<_> = settings
            .workspaces
            .iter()
            .map(|w| w.path.to_string())
            .collect();
        assert_eq!(paths, vec!["/a", "/b"]);
        assert!(
            !settings.workspaces[0].auto_fast_forward,
            "existing opt-out must survive"
        );
    }

    #[tokio::test]
    async fn setup_with_a_relative_path_is_rejected_and_writes_nothing() {
        let dir = tempfile::tempdir().expect("BUG");
        let store = TomlSettings::new(dir.path().join("config.toml"), PathBuf::from(HOME));
        let error = store
            .apply_setup(&setup(&["projects"], IconPreference::Unicode))
            .await
            .unwrap_err();
        assert!(matches!(error, SettingsError::Invalid { .. }), "{error}");
        assert!(!store.path().exists());
    }

    #[tokio::test]
    async fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().expect("BUG: temp dir");
        let store = TomlSettings::new(dir.path().join("config.toml"), PathBuf::from(HOME));
        assert_eq!(store.load().await.expect("BUG: load"), Settings::default());
    }
}
