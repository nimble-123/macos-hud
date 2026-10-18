//! `config.toml` adapter for the [`SettingsStore`] port.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hud_application::settings::{
    AutoInstallKind, AutoInstallSettings, Billing, IconPreference, InitialSetup,
    LanguagePreference, OverviewInsight, OverviewSettings, ProviderSettings, ScanSettings,
    Settings, SettingsChange, SettingsError, SettingsStore, SkillsSettings, TelemetrySettings,
    ThemePreference, ToolPaths, WorkspaceSettings,
};
use hud_domain::WorkspacePath;
use hud_domain::usage::Provider as AiProvider;
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
        self.edit(|existing| with_setup(existing, setup, &self.home))
            .await
    }

    async fn update(&self, change: &SettingsChange) -> Result<Settings, SettingsError> {
        self.edit(|existing| with_change(existing, change, &self.home))
            .await
    }
}

impl TomlSettings {
    /// Reads the file as it is now, so hand edits made while hud runs are
    /// kept, applies `change` and writes the result atomically.
    async fn edit(
        &self,
        change: impl FnOnce(Option<&str>) -> Result<String, String>,
    ) -> Result<Settings, SettingsError> {
        let invalid = |message| SettingsError::Invalid {
            path: self.path.display().to_string(),
            message,
        };
        let existing = self.read_text().await?;
        let text = change(existing.as_deref()).map_err(invalid)?;
        // Validate before writing: a change that produces an invalid file is rejected.
        let settings = parse(&text, &self.home).map_err(invalid)?;
        self.write_atomically(&text).await?;
        Ok(settings)
    }

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
fn with_setup(existing: Option<&str>, setup: &InitialSetup, home: &Path) -> Result<String, String> {
    let mut document = document(existing)?;
    document["icons"] = toml_edit::value(icons_value(setup.icons));
    set(
        &mut section(&mut document, "ai", "claude")["enabled"],
        setup.claude,
    );
    set(
        &mut section(&mut document, "ai", "codex")["enabled"],
        setup.codex,
    );
    for path in &setup.workspaces {
        add_workspace(&mut document, path, home)?;
    }
    Ok(render(existing, &document))
}

/// Applies one settings change; only the keys it is about are touched.
fn with_change(
    existing: Option<&str>,
    change: &SettingsChange,
    home: &Path,
) -> Result<String, String> {
    let mut document = document(existing)?;
    match change {
        SettingsChange::Language(language) => {
            set(
                &mut document["language"],
                match language {
                    LanguagePreference::Auto => "auto",
                    LanguagePreference::De => "de",
                    LanguagePreference::En => "en",
                },
            );
        }
        SettingsChange::Theme(theme) => {
            set(
                &mut document["theme"],
                match theme {
                    ThemePreference::Auto => "auto",
                    ThemePreference::Dark => "dark",
                    ThemePreference::Light => "light",
                },
            );
        }
        SettingsChange::Icons(icons) => set(&mut document["icons"], icons_value(*icons)),
        SettingsChange::AddWorkspace(path) => add_workspace(&mut document, path, home)?,
        SettingsChange::RemoveWorkspace(workspace) => {
            let tables = workspaces_mut(&mut document)?;
            let index = workspace_index(tables, workspace.as_path(), home)?;
            tables.remove(index);
        }
        SettingsChange::AutoFastForward { workspace, enabled } => {
            let tables = workspaces_mut(&mut document)?;
            let index = workspace_index(tables, workspace.as_path(), home)?;
            if let Some(table) = tables.get_mut(index) {
                set(&mut table["auto_fast_forward"], *enabled);
            }
        }
        SettingsChange::AutoInstall { kind, enabled } => {
            let (table, key) = match kind {
                AutoInstallKind::BrewFormulae => ("brew", "auto_install_formulae"),
                AutoInstallKind::BrewCasks => ("brew", "auto_install_casks"),
                AutoInstallKind::Npm => ("npm", "auto_install"),
            };
            set(&mut section(&mut document, "updates", table)[key], *enabled);
        }
        SettingsChange::AiSource { provider, enabled } => {
            let table = match provider {
                AiProvider::Claude => "claude",
                AiProvider::Codex => "codex",
            };
            set(
                &mut section(&mut document, "ai", table)["enabled"],
                *enabled,
            );
        }
        SettingsChange::OverviewInsight { insight, enabled } => {
            let key = match insight {
                OverviewInsight::Unpushed => "unpushed",
                OverviewInsight::UpdateAttention => "update_attention",
                OverviewInsight::Problems => "problems",
            };
            set(&mut table(&mut document, "overview")[key], *enabled);
        }
    }
    Ok(render(existing, &document))
}

/// `[<name>]`, created as a section of its own like [`section`].
fn table<'a>(document: &'a mut toml_edit::DocumentMut, name: &str) -> &'a mut toml_edit::Item {
    document
        .entry(name)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
}

/// `[<parent>.<name>]`, created as a section of its own: indexing would
/// create an inline table such as `ai = { codex = { enabled = true } }`.
fn section<'a>(
    document: &'a mut toml_edit::DocumentMut,
    parent: &str,
    name: &str,
) -> &'a mut toml_edit::Item {
    let outer = document.entry(parent).or_insert_with(|| {
        let mut table = toml_edit::Table::new();
        table.set_implicit(true);
        toml_edit::Item::Table(table)
    });
    if let Some(table) = outer.as_table_mut()
        && !table.contains_key(name)
    {
        table.insert(name, toml_edit::Item::Table(toml_edit::Table::new()));
    }
    &mut outer[name]
}

/// Replaces a value but keeps its surrounding whitespace and inline comment.
fn set(item: &mut toml_edit::Item, value: impl Into<toml_edit::Value>) {
    let mut value = value.into();
    if let Some(old) = item.as_value() {
        *value.decor_mut() = old.decor().clone();
    }
    *item = toml_edit::Item::Value(value);
}

fn document(existing: Option<&str>) -> Result<toml_edit::DocumentMut, String> {
    existing
        .unwrap_or_default()
        .parse()
        .map_err(|error: toml_edit::TomlError| error.to_string())
}

/// `toml_edit` attaches comments of an empty document to its end, so the
/// header of a new file is prepended to the rendered text instead.
fn render(existing: Option<&str>, document: &toml_edit::DocumentMut) -> String {
    match existing {
        Some(_) => document.to_string(),
        None => format!("{NEW_FILE_HEADER}{document}"),
    }
}

fn icons_value(icons: IconPreference) -> &'static str {
    match icons {
        IconPreference::Unicode => "unicode",
        IconPreference::Nerd => "nerd",
        IconPreference::Ascii => "ascii",
    }
}

fn workspaces_mut(
    document: &mut toml_edit::DocumentMut,
) -> Result<&mut toml_edit::ArrayOfTables, String> {
    document
        .entry("workspaces")
        .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| "workspaces must be an array of tables ([[workspaces]])".to_owned())
}

/// Position of the workspace whose path, after `~/` expansion, is `path`.
fn workspace_index(
    tables: &toml_edit::ArrayOfTables,
    path: &Path,
    home: &Path,
) -> Result<usize, String> {
    tables
        .iter()
        .position(|table| {
            table
                .get("path")
                .and_then(toml_edit::Item::as_str)
                .is_some_and(|written| expand_home(written, home) == path)
        })
        .ok_or_else(|| format!("workspace {} is not in the config", path.display()))
}

/// Appends a workspace unless one with the same expanded path exists.
fn add_workspace(
    document: &mut toml_edit::DocumentMut,
    path: &str,
    home: &Path,
) -> Result<(), String> {
    let tables = workspaces_mut(document)?;
    if workspace_index(tables, &expand_home(path, home), home).is_ok() {
        return Ok(());
    }
    let mut table = toml_edit::Table::new();
    table["path"] = toml_edit::value(path);
    table["auto_fast_forward"] = toml_edit::value(true);
    tables.push(table);
    Ok(())
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
    overview: Overview,
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
    telemetry: Telemetry,
    prices: BTreeMap<String, Price>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Telemetry {
    enabled: bool,
    port: u16,
}

impl Default for Telemetry {
    fn default() -> Self {
        let defaults = TelemetrySettings::default();
        Self {
            enabled: defaults.enabled,
            port: defaults.port,
        }
    }
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
    cache_write_5m: f64,
    cache_write_1h: f64,
    cache_read: f64,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Overview {
    unpushed: bool,
    update_attention: bool,
    problems: bool,
}

impl Default for Overview {
    fn default() -> Self {
        let defaults = OverviewSettings::default();
        Self {
            unpushed: defaults.unpushed,
            update_attention: defaults.update_attention,
            problems: defaults.problems,
        }
    }
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
            telemetry: TelemetrySettings {
                enabled: self.ai.telemetry.enabled,
                port: self.ai.telemetry.port,
            },
            overview: OverviewSettings {
                unpushed: self.overview.unpushed,
                update_attention: self.overview.update_attention,
                problems: self.overview.problems,
            },
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
    fn to_micro_usd(&self, model: &str) -> Result<hud_domain::usage::Price, String> {
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
        Ok(hud_domain::usage::Price {
            input: convert(self.input, "input")?,
            output: convert(self.output, "output")?,
            cache_write_5m: convert(self.cache_write_5m, "cache_write_5m")?,
            cache_write_1h: convert(self.cache_write_1h, "cache_write_1h")?,
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
cache_write_5m = 3.75
cache_write_1h = 6.0
cache_read = 0.3

[overview]
unpushed = false
update_attention = false
problems = false

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
        assert!(!settings.overview.unpushed);
        assert!(!settings.overview.update_attention);
        assert!(!settings.overview.problems);
        assert_eq!(settings.prices["claude-opus-5-5"].cache_write_5m, 3_750_000);
        assert_eq!(settings.prices["claude-opus-5-5"].cache_write_1h, 6_000_000);
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
        let text = "[ai.prices.m]\ninput = -1.0\noutput = 0.0\ncache_write_5m = 0.0\ncache_write_1h = 0.0\ncache_read = 0.0\n";
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
            claude: true,
            codex: false,
        }
    }

    #[tokio::test]
    async fn setup_writes_the_chosen_ai_sources() {
        let dir = tempfile::tempdir().expect("BUG");
        let store = TomlSettings::new(dir.path().join("hud/config.toml"), PathBuf::from(HOME));
        let mut choice = setup(&["~/workspace"], IconPreference::Unicode);
        choice.claude = false;
        choice.codex = true;
        let applied = store.apply_setup(&choice).await.expect("BUG");
        assert!(!applied.claude.enabled);
        assert!(applied.codex.enabled);
    }

    #[test]
    fn an_opt_in_gets_a_section_of_its_own() {
        let (text, settings) = change(&SettingsChange::AutoInstall {
            kind: AutoInstallKind::Npm,
            enabled: true,
        });
        assert!(settings.auto_install.npm);
        assert!(
            text.contains("[updates.npm]\nauto_install = true"),
            "{text}"
        );
        assert!(!text.contains("updates = {"), "{text}");
    }

    #[test]
    fn switching_an_ai_source_touches_only_its_key() {
        let (text, settings) = change(&SettingsChange::AiSource {
            provider: AiProvider::Codex,
            enabled: true,
        });
        assert!(settings.codex.enabled);
        assert!(settings.claude.enabled, "Claude Code keeps its default");
        assert!(text.contains("[ai.codex]\nenabled = true"), "{text}");
    }

    #[test]
    fn an_overview_insight_is_switched_in_its_own_section() {
        let (text, settings) = change(&SettingsChange::OverviewInsight {
            insight: OverviewInsight::Unpushed,
            enabled: false,
        });
        assert!(!settings.overview.unpushed);
        assert!(text.contains("[overview]\nunpushed = false"), "{text}");
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

    const HAND_WRITTEN: &str = "\
# my comment
language = \"de\" # inline comment

[[workspaces]]
path = \"~/code\"
auto_fast_forward = true

[[workspaces]]
path = \"/srv/repos\"
";

    fn workspace_path(path: &str) -> WorkspacePath {
        WorkspacePath::try_from(PathBuf::from(path)).expect("BUG: absolute")
    }

    fn change(change: &SettingsChange) -> (String, Settings) {
        let text = with_change(Some(HAND_WRITTEN), change, Path::new(HOME)).expect("BUG: valid");
        let settings = parse(&text, Path::new(HOME)).expect("BUG: parses");
        assert!(text.starts_with("# my comment"), "{text}");
        (text, settings)
    }

    #[test]
    fn changing_the_theme_keeps_comments_and_other_keys() {
        let (text, settings) = change(&SettingsChange::Theme(ThemePreference::Light));
        assert_eq!(settings.theme, ThemePreference::Light);
        assert!(
            text.contains("language = \"de\" # inline comment"),
            "{text}"
        );
    }

    #[test]
    fn changing_the_language_keeps_its_inline_comment() {
        let (text, settings) = change(&SettingsChange::Language(LanguagePreference::En));
        assert_eq!(settings.language, LanguagePreference::En);
        assert!(text.contains("# inline comment"), "{text}");
    }

    #[test]
    fn adding_a_known_workspace_in_another_spelling_changes_nothing() {
        let path = format!("{HOME}/code");
        let (text, _) = change(&SettingsChange::AddWorkspace(path));
        assert_eq!(text, HAND_WRITTEN);
    }

    #[test]
    fn adding_a_workspace_appends_it_with_fast_forward_on() {
        let (_, settings) = change(&SettingsChange::AddWorkspace("~/new".into()));
        let added = settings.workspaces.last().expect("BUG: three workspaces");
        assert_eq!(added.path, workspace_path(&format!("{HOME}/new")));
        assert!(added.auto_fast_forward);
    }

    #[test]
    fn removing_a_workspace_matches_the_expanded_path() {
        let workspace = workspace_path(&format!("{HOME}/code"));
        let (_, settings) = change(&SettingsChange::RemoveWorkspace(workspace));
        assert_eq!(settings.workspaces.len(), 1);
        assert_eq!(settings.workspaces[0].path, workspace_path("/srv/repos"));
    }

    #[test]
    fn removing_an_unknown_workspace_is_an_error() {
        let change = SettingsChange::RemoveWorkspace(workspace_path("/nowhere"));
        assert!(with_change(Some(HAND_WRITTEN), &change, Path::new(HOME)).is_err());
    }

    #[test]
    fn fast_forward_opt_out_is_written_to_its_workspace() {
        let (_, settings) = change(&SettingsChange::AutoFastForward {
            workspace: workspace_path("/srv/repos"),
            enabled: false,
        });
        assert!(settings.workspaces[0].auto_fast_forward);
        assert!(!settings.workspaces[1].auto_fast_forward);
    }

    #[test]
    fn an_opt_in_writes_only_its_own_key() {
        let (text, settings) = change(&SettingsChange::AutoInstall {
            kind: AutoInstallKind::BrewCasks,
            enabled: true,
        });
        assert!(settings.auto_install.brew_casks);
        assert!(text.contains("auto_install_casks = true"), "{text}");
        assert!(!text.contains("auto_install_formulae"), "{text}");
        assert!(!text.contains("[updates.npm]"), "{text}");
    }

    #[tokio::test]
    async fn update_reads_the_file_again_and_keeps_hand_edits() {
        let dir = tempfile::tempdir().expect("BUG");
        let store = TomlSettings::new(dir.path().join("config.toml"), PathBuf::from(HOME));
        std::fs::write(store.path(), HAND_WRITTEN).expect("BUG");
        let _ = store.load().await.expect("BUG");
        // Edited by hand while hud runs.
        std::fs::write(
            store.path(),
            format!("{HAND_WRITTEN}\n[git]\nfetch_interval_min = 5\n"),
        )
        .expect("BUG");

        let settings = store
            .update(&SettingsChange::Icons(IconPreference::Ascii))
            .await
            .expect("BUG");
        assert_eq!(settings.icons, IconPreference::Ascii);
        assert_eq!(settings.fetch_interval, Duration::from_mins(5));
    }

    #[test]
    fn setup_keeps_existing_keys_comments_and_workspaces() {
        let existing = "# my comment\nlanguage = \"de\"\n\n[[workspaces]]\npath = \"/a\"\nauto_fast_forward = false\n";
        let text = with_setup(
            Some(existing),
            &setup(&["/a", "/b"], IconPreference::Unicode),
            Path::new(HOME),
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
