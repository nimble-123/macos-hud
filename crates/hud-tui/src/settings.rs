//! Presentation settings resolved once at start-up.
//!
//! The binary reads the environment and the config; these types and their
//! pure constructors decide what the values mean.

/// UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// German.
    De,
    /// English, also the fallback.
    #[default]
    En,
}

impl Language {
    /// Picks the language from a BCP 47 locale such as `de-DE` or `en_US.UTF-8`.
    #[must_use]
    pub fn from_locale(locale: &str) -> Self {
        let primary = locale.split(['-', '_', '.']).next().unwrap_or_default();
        if primary.eq_ignore_ascii_case("de") {
            Self::De
        } else {
            Self::En
        }
    }
}

/// Light or dark terminal background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    /// Dark background; also the fallback when detection fails.
    #[default]
    Dark,
    /// Light background.
    Light,
}

/// How many colours the terminal can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    /// 24-bit colour.
    TrueColor,
    /// xterm 256-colour palette.
    Ansi256,
    /// The 16 basic ANSI colours.
    Ansi16,
    /// No colour at all (`NO_COLOR`).
    None,
}

impl ColorDepth {
    /// Derives the depth from `NO_COLOR`, `COLORTERM` and `TERM`.
    ///
    /// `NO_COLOR` wins whenever it is set to a non-empty value, as the
    /// convention at <https://no-color.org> requires.
    #[must_use]
    pub fn from_env(no_color: Option<&str>, colorterm: Option<&str>, term: Option<&str>) -> Self {
        if no_color.is_some_and(|value| !value.is_empty()) {
            return Self::None;
        }
        if colorterm.is_some_and(|value| value == "truecolor" || value == "24bit") {
            return Self::TrueColor;
        }
        if term.is_some_and(|value| value.contains("256color")) {
            return Self::Ansi256;
        }
        Self::Ansi16
    }
}

/// Glyph set for status and source symbols (see `docs/DESIGN.md` §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSet {
    /// Geometric Unicode symbols that every terminal font has.
    #[default]
    Unicode,
    /// Nerd Font icons; needs a Nerd Font in the terminal.
    Nerd,
    /// Plain ASCII for minimal terminals.
    Ascii,
}

/// All presentation settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSettings {
    /// UI language.
    pub language: Language,
    /// Terminal background.
    pub appearance: Appearance,
    /// Colour capability of the terminal.
    pub color_depth: ColorDepth,
    /// Glyph set.
    pub icons: IconSet,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            language: Language::En,
            appearance: Appearance::Dark,
            color_depth: ColorDepth::TrueColor,
            icons: IconSet::Unicode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_locales_select_german() {
        for locale in ["de", "de-DE", "de_AT.UTF-8", "DE-ch"] {
            assert_eq!(Language::from_locale(locale), Language::De, "{locale}");
        }
    }

    #[test]
    fn other_locales_fall_back_to_english() {
        for locale in ["en-US", "fr-FR", "", "C"] {
            assert_eq!(Language::from_locale(locale), Language::En, "{locale}");
        }
    }

    #[test]
    fn no_color_overrides_everything() {
        let depth = ColorDepth::from_env(Some("1"), Some("truecolor"), Some("xterm-256color"));
        assert_eq!(depth, ColorDepth::None);
    }

    #[test]
    fn empty_no_color_is_ignored() {
        let depth = ColorDepth::from_env(Some(""), Some("truecolor"), None);
        assert_eq!(depth, ColorDepth::TrueColor);
    }

    #[test]
    fn colorterm_truecolor_means_true_color() {
        assert_eq!(
            ColorDepth::from_env(None, Some("24bit"), None),
            ColorDepth::TrueColor
        );
    }

    #[test]
    fn term_256color_means_256_colors() {
        let depth = ColorDepth::from_env(None, None, Some("xterm-256color"));
        assert_eq!(depth, ColorDepth::Ansi256);
    }

    #[test]
    fn unknown_terminal_gets_16_colors() {
        assert_eq!(
            ColorDepth::from_env(None, None, Some("xterm")),
            ColorDepth::Ansi16
        );
    }
}
