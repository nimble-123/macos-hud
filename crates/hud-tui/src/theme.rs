//! Colour roles from `docs/DESIGN.md` §3, resolved for appearance and colour depth.

use ratatui::style::Color;

use crate::icons::Status;
use crate::settings::{Appearance, ColorDepth};

/// Resolved colours for every role. Built once at start-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub(crate) ink: Color,
    pub(crate) muted: Color,
    pub(crate) soft: Color,
    pub(crate) focus: Color,
    pub(crate) rule: Color,
    ok: Color,
    advisory: Color,
    caution: Color,
    warning: Color,
    /// Five steps from `rule` to `advisory` for the activity calendar.
    heat: [Color; 5],
}

/// Hex values per role: (dark, light).
struct Rgb(u8, u8, u8);
const INK: (Rgb, Rgb) = (Rgb(0xE6, 0xE8, 0xE3), Rgb(0x1C, 0x21, 0x28));
const MUTED: (Rgb, Rgb) = (Rgb(0xAE, 0xB5, 0xBD), Rgb(0x47, 0x50, 0x5C));
const SOFT: (Rgb, Rgb) = (Rgb(0x8F, 0x98, 0xA3), Rgb(0x5A, 0x64, 0x70));
const FOCUS: (Rgb, Rgb) = (Rgb(0xE8, 0x70, 0xC6), Rgb(0xB0, 0x2A, 0x8A));
const RULE: (Rgb, Rgb) = (Rgb(0x3A, 0x42, 0x4D), Rgb(0xC4, 0xC8, 0xC2));
const OK: (Rgb, Rgb) = (Rgb(0x5C, 0xC9, 0x8A), Rgb(0x24, 0x77, 0x4A));
const ADVISORY: (Rgb, Rgb) = (Rgb(0x4C, 0xC0, 0xD6), Rgb(0x13, 0x73, 0x8A));
const CAUTION: (Rgb, Rgb) = (Rgb(0xE8, 0xB2, 0x3A), Rgb(0x8F, 0x62, 0x00));
const WARNING: (Rgb, Rgb) = (Rgb(0xF0, 0x67, 0x5B), Rgb(0xB4, 0x2F, 0x25));
/// Percent of the way from `rule` to `advisory` per activity level.
const HEAT_STEPS: [u16; 5] = [0, 35, 60, 80, 100];

impl Theme {
    /// Resolves every role for the given background and colour depth.
    #[must_use]
    pub fn new(appearance: Appearance, depth: ColorDepth) -> Self {
        let pick = |pair: &(Rgb, Rgb), ansi: Color| -> Color {
            let Rgb(r, g, b) = match appearance {
                Appearance::Dark => &pair.0,
                Appearance::Light => &pair.1,
            };
            match depth {
                ColorDepth::TrueColor => Color::Rgb(*r, *g, *b),
                ColorDepth::Ansi256 => Color::Indexed(nearest_xterm(*r, *g, *b)),
                ColorDepth::Ansi16 => ansi,
                ColorDepth::None => Color::Reset,
            }
        };
        let (rule, advisory) = match appearance {
            Appearance::Dark => (&RULE.0, &ADVISORY.0),
            Appearance::Light => (&RULE.1, &ADVISORY.1),
        };
        let heat = HEAT_STEPS.map(|step| {
            let mix = |from: u8, to: u8| {
                let value = (u16::from(from) * (100 - step) + u16::from(to) * step) / 100;
                u8::try_from(value).unwrap_or(u8::MAX)
            };
            let (r, g, b) = (
                mix(rule.0, advisory.0),
                mix(rule.1, advisory.1),
                mix(rule.2, advisory.2),
            );
            match depth {
                ColorDepth::TrueColor => Color::Rgb(r, g, b),
                ColorDepth::Ansi256 => Color::Indexed(nearest_xterm(r, g, b)),
                ColorDepth::Ansi16 => match step {
                    0 => Color::DarkGray,
                    1..=60 => Color::Blue,
                    61..=80 => Color::Cyan,
                    _ => Color::LightCyan,
                },
                ColorDepth::None => Color::Reset,
            }
        });
        Self {
            heat,
            ink: pick(&INK, Color::Reset),
            muted: pick(&MUTED, Color::DarkGray),
            soft: pick(&SOFT, Color::DarkGray),
            focus: pick(&FOCUS, Color::Magenta),
            rule: pick(&RULE, Color::DarkGray),
            ok: pick(&OK, Color::Green),
            advisory: pick(&ADVISORY, Color::Cyan),
            caution: pick(&CAUTION, Color::Yellow),
            warning: pick(&WARNING, Color::Red),
        }
    }

    /// Colour of an activity level from 0 (none) to 4 (most).
    #[must_use]
    pub fn heat(&self, level: u8) -> Color {
        self.heat[usize::from(level.min(4))]
    }

    /// Whether colours are shown at all; without them levels need glyphs.
    #[must_use]
    pub fn colorful(&self) -> bool {
        self.ink != Color::Reset || self.advisory != Color::Reset
    }

    /// Colour of a status level; `neutral` uses the muted role.
    #[must_use]
    pub fn status(&self, status: Status) -> Color {
        match status {
            Status::Ok => self.ok,
            Status::Advisory => self.advisory,
            Status::Caution => self.caution,
            Status::Warning => self.warning,
            Status::Neutral => self.muted,
        }
    }
}

/// Nearest entry of the xterm 256-colour palette (6×6×6 cube or grey ramp).
fn nearest_xterm(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |c: u8| -> usize {
        let mut best = 0;
        for (i, l) in LEVELS.iter().enumerate() {
            if c.abs_diff(*l) < c.abs_diff(LEVELS[best]) {
                best = i;
            }
        }
        best
    };
    let distance = |a: (u8, u8, u8)| -> u32 {
        let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
        d(a.0, r) + d(a.1, g) + d(a.2, b)
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    // Indices are at most 5, so the cube index fits into u8 (16 + 215 = 231).
    let cube_index = 16 + 36 * ri + 6 * gi + bi;

    let average = (u32::from(r) + u32::from(g) + u32::from(b)) / 3;
    let gray_step = (average.saturating_sub(8) / 10).min(23);
    let gray_value = u8::try_from(8 + 10 * gray_step).unwrap_or(u8::MAX);
    let gray = (gray_value, gray_value, gray_value);
    let gray_index = 232 + gray_step;

    let index = if distance(gray) < distance(cube) {
        usize::try_from(gray_index).unwrap_or(255)
    } else {
        cube_index
    };
    u8::try_from(index).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn true_color_uses_exact_design_values() {
        let theme = Theme::new(Appearance::Dark, ColorDepth::TrueColor);
        assert_eq!(theme.focus, Color::Rgb(0xE8, 0x70, 0xC6));
    }

    #[test]
    fn light_appearance_uses_light_values() {
        let theme = Theme::new(Appearance::Light, ColorDepth::TrueColor);
        assert_eq!(theme.focus, Color::Rgb(0xB0, 0x2A, 0x8A));
    }

    #[test]
    fn sixteen_colors_follow_the_design_mapping() {
        let theme = Theme::new(Appearance::Dark, ColorDepth::Ansi16);
        assert_eq!(theme.focus, Color::Magenta);
        assert_eq!(theme.status(Status::Caution), Color::Yellow);
    }

    #[test]
    fn no_color_resets_every_role() {
        let theme = Theme::new(Appearance::Dark, ColorDepth::None);
        assert_eq!(theme.focus, Color::Reset);
        assert_eq!(theme.status(Status::Warning), Color::Reset);
    }

    #[test]
    fn nearest_xterm_maps_pure_colors_exactly() {
        assert_eq!(nearest_xterm(0, 0, 0), 16);
        assert_eq!(nearest_xterm(255, 255, 255), 231);
        assert_eq!(nearest_xterm(255, 0, 0), 196);
    }

    #[test]
    fn nearest_xterm_prefers_grey_ramp_for_greys() {
        assert_eq!(nearest_xterm(0x3A, 0x3A, 0x3A), 237);
    }
}
