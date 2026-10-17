//! Resolves presentation settings from the environment and the terminal.

use std::io::IsTerminal;
use std::time::Duration;

use hud_tui::{Appearance, ColorDepth, IconSet, Language, UiSettings};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

/// OSC 11 answer deadline; terminals without support are usually detected sooner.
const THEME_QUERY_TIMEOUT: Duration = Duration::from_millis(100);

/// Must run before the terminal enters raw mode: the OSC 11 query reads the
/// terminal's reply from stdin.
pub fn ui_settings() -> UiSettings {
    let settings = UiSettings {
        language: sys_locale::get_locale()
            .map(|locale| Language::from_locale(&locale))
            .unwrap_or_default(),
        appearance: detect_appearance(),
        color_depth: ColorDepth::from_env(
            std::env::var("NO_COLOR").ok().as_deref(),
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        ),
        icons: IconSet::default(),
    };
    tracing::info!(?settings, "resolved presentation settings");
    settings
}

fn detect_appearance() -> Appearance {
    if !std::io::stdout().is_terminal() {
        return Appearance::default();
    }
    let mut options = QueryOptions::default();
    options.timeout = THEME_QUERY_TIMEOUT;
    match theme_mode(options) {
        Ok(ThemeMode::Light) => Appearance::Light,
        Ok(ThemeMode::Dark) => Appearance::Dark,
        Err(error) => {
            tracing::debug!(%error, "terminal background not detected, assuming dark");
            Appearance::default()
        }
    }
}
