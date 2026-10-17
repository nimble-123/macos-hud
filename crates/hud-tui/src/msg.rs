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
    /// The terminal was resized; the next frame adapts automatically.
    Resize,
    /// Render tick for time-based UI such as spinners; carries the current
    /// time so that `update` stays free of clock access.
    Tick(Instant),
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
            Event::Resize(..) => Some(Self::Resize),
            Event::Key(_) | Event::FocusGained | Event::FocusLost | Event::Paste(_) => None,
        }
    }
}
