//! UI state.

/// Top-level tabs, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Overview,
    Workspaces,
    Updates,
    Ai,
    Log,
    Settings,
}

impl Tab {
    /// All tabs in display order.
    pub const ALL: [Self; 6] = [
        Self::Overview,
        Self::Workspaces,
        Self::Updates,
        Self::Ai,
        Self::Log,
        Self::Settings,
    ];

    /// Position of the tab in [`Tab::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Workspaces => 1,
            Self::Updates => 2,
            Self::Ai => 3,
            Self::Log => 4,
            Self::Settings => 5,
        }
    }

    /// The tab to the right, wrapping around.
    #[must_use]
    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// The tab to the left, wrapping around.
    #[must_use]
    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Complete UI state. Changed only by [`crate::update`].
#[derive(Debug, Default)]
pub struct Model {
    pub(crate) tab: Tab,
    pub(crate) quit: bool,
}

impl Model {
    /// The active tab.
    #[must_use]
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Whether the event loop should stop.
    #[must_use]
    pub fn should_quit(&self) -> bool {
        self.quit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wraps_from_last_to_first() {
        assert_eq!(Tab::Settings.next(), Tab::Overview);
    }

    #[test]
    fn previous_wraps_from_first_to_last() {
        assert_eq!(Tab::Overview.previous(), Tab::Settings);
    }

    #[test]
    fn index_matches_position_in_all() {
        for (position, tab) in Tab::ALL.iter().enumerate() {
            assert_eq!(tab.index(), position);
        }
    }
}
