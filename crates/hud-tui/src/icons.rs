//! Status and source glyphs per [`IconSet`] (`docs/DESIGN.md` §3.4).

use crate::settings::IconSet;

/// Status levels following the EICAS colour code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Just done.
    Ok,
    /// Something is available or behind.
    Advisory,
    /// Skipped or needs a manual step.
    Caution,
    /// Error or diverged.
    Warning,
    /// Nothing to do.
    Neutral,
}

/// Things that get their own icon in the `nerd` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A Git repository.
    Git,
    /// Homebrew.
    Homebrew,
    /// npm.
    Npm,
    /// Skills CLI.
    Skills,
    /// An AI provider.
    Ai,
    /// CPU load.
    Cpu,
    /// Memory.
    Memory,
}

/// Glyph for a status level.
#[must_use]
pub fn status_glyph(icons: IconSet, status: Status) -> &'static str {
    match (icons, status) {
        (IconSet::Unicode, Status::Ok) => "●",
        (IconSet::Unicode, Status::Advisory) => "◆",
        (IconSet::Unicode, Status::Caution) => "▲",
        (IconSet::Unicode, Status::Warning) => "✖",
        (IconSet::Unicode, Status::Neutral) => "○",
        (IconSet::Nerd, Status::Ok) => "\u{f058}", // nf-fa-check_circle
        (IconSet::Nerd, Status::Advisory) => "\u{f05a}", // nf-fa-info_circle
        (IconSet::Nerd, Status::Caution) => "\u{f071}", // nf-fa-exclamation_triangle
        (IconSet::Nerd, Status::Warning) => "\u{f057}", // nf-fa-times_circle
        (IconSet::Nerd, Status::Neutral) => "\u{f10c}", // nf-fa-circle_o
        (IconSet::Ascii, Status::Ok) => "+",
        (IconSet::Ascii, Status::Advisory) => "i",
        (IconSet::Ascii, Status::Caution) => "!",
        (IconSet::Ascii, Status::Warning) => "x",
        (IconSet::Ascii, Status::Neutral) => "-",
    }
}

/// Icon for a source; only the `nerd` set has them, the others show the name.
#[must_use]
pub fn source_glyph(icons: IconSet, source: Source) -> Option<&'static str> {
    match icons {
        IconSet::Nerd => Some(match source {
            Source::Git => "\u{e725}",      // nf-dev-git_branch
            Source::Homebrew => "\u{f0fc}", // nf-fa-beer
            Source::Npm => "\u{e71e}",      // nf-dev-npm
            Source::Skills => "\u{f12e}",   // nf-fa-puzzle_piece
            Source::Ai => "\u{f06a9}",      // nf-md-robot
            Source::Cpu => "\u{f4bc}",      // nf-oct-cpu
            Source::Memory => "\u{f035b}",  // nf-md-memory
        }),
        IconSet::Unicode | IconSet::Ascii => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATUS: [Status; 5] = [
        Status::Ok,
        Status::Advisory,
        Status::Caution,
        Status::Warning,
        Status::Neutral,
    ];

    #[test]
    fn every_set_has_distinct_status_glyphs() {
        for icons in [IconSet::Unicode, IconSet::Nerd, IconSet::Ascii] {
            let mut glyphs: Vec<_> = ALL_STATUS.iter().map(|s| status_glyph(icons, *s)).collect();
            glyphs.sort_unstable();
            glyphs.dedup();
            assert_eq!(glyphs.len(), ALL_STATUS.len(), "{icons:?}");
        }
    }

    #[test]
    fn ascii_glyphs_are_ascii() {
        for status in ALL_STATUS {
            assert!(status_glyph(IconSet::Ascii, status).is_ascii());
        }
    }

    #[test]
    fn only_nerd_set_has_source_icons() {
        assert!(source_glyph(IconSet::Nerd, Source::Git).is_some());
        assert!(source_glyph(IconSet::Unicode, Source::Git).is_none());
        assert!(source_glyph(IconSet::Ascii, Source::Git).is_none());
    }
}
