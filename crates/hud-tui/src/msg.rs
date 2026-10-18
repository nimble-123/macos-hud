//! Messages that drive [`crate::update`].

use std::time::Instant;

use crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEvent};
use hud_application::AppEvent;

/// Everything the UI reacts to.
#[derive(Debug)]
pub enum Msg {
    /// A key was pressed.
    Key(KeyEvent),
    /// A mouse button, wheel or movement event.
    Mouse(MouseEvent),
    /// The terminal has this many columns and rows; sent at start and on
    /// every resize, so clicks resolve against the drawn layout.
    Resize(u16, u16),
    /// Render tick for time-based UI such as spinners; carries the current
    /// time, monotonic and wall clock, so that `update` stays free of clock access.
    Tick(Instant, jiff::Timestamp),
    /// Result of background work.
    App(AppEvent),
}

impl Msg {
    /// Converts a terminal event into a message, dropping events the UI ignores.
    ///
    /// Key releases and repeats are ignored so that a key acts exactly once.
    #[must_use]
    pub fn from_terminal(event: &Event) -> Option<Self> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => Some(Self::Key(*key)),
            Event::Mouse(mouse) => Some(Self::Mouse(*mouse)),
            Event::Resize(columns, rows) => Some(Self::Resize(*columns, *rows)),
            Event::Key(_) | Event::FocusGained | Event::FocusLost | Event::Paste(_) => None,
        }
    }
}
