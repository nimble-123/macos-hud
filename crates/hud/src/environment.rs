//! Detects system language, terminal background and colour depth.

use std::io::IsTerminal;
use std::time::Duration;

use hud_tui::{Appearance, ColorDepth, Language, System};
use terminal_colorsaurus::{QueryOptions, ThemeMode, theme_mode};

/// OSC 11 answer deadline; terminals without support are usually detected sooner.
const THEME_QUERY_TIMEOUT: Duration = Duration::from_millis(100);

/// System language and terminal background, detected even when the config
/// overrides them, so switching to `auto` later needs no second query.
///
/// Must run before the terminal enters raw mode: the OSC 11 query reads the
/// terminal's reply from stdin.
pub fn system() -> System {
    let system = System {
        language: sys_locale::get_locale()
            .map(|locale| Language::from_locale(&locale))
            .unwrap_or_default(),
        appearance: detect_appearance(),
    };
    tracing::info!(?system, "detected system settings");
    system
}

/// Colour capability from `NO_COLOR`, `COLORTERM` and `TERM`.
pub fn color_depth() -> ColorDepth {
    ColorDepth::from_env(
        std::env::var("NO_COLOR").ok().as_deref(),
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    )
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
