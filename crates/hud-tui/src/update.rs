//! State transitions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hud_application::Command;

use crate::{Model, Msg};

/// Applies a message to the model and returns the commands to run.
///
/// Pure: no I/O and no clock, so every transition is unit-testable.
pub fn update(model: &mut Model, msg: Msg) -> Vec<Command> {
    match msg {
        Msg::Key(key) => on_key(model, key),
        Msg::App(event) => match event {},
        Msg::Mouse(_) | Msg::Resize | Msg::Tick => {}
    }
    Vec::new()
}

fn on_key(model: &mut Model, key: KeyEvent) {
    match key.code {
        KeyCode::Tab => model.tab = model.tab.next(),
        KeyCode::BackTab => model.tab = model.tab.previous(),
        KeyCode::Char('q') => model.quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => model.quit = true,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tab;

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn tab_moves_to_next_tab() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Tab));
        assert_eq!(model.tab(), Tab::Workspaces);
    }

    #[test]
    fn back_tab_moves_to_previous_tab() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::BackTab));
        assert_eq!(model.tab(), Tab::Settings);
    }

    #[test]
    fn q_requests_quit() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Char('q')));
        assert!(model.should_quit());
    }

    #[test]
    fn ctrl_c_requests_quit() {
        let mut model = Model::default();
        update(
            &mut model,
            Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        );
        assert!(model.should_quit());
    }

    #[test]
    fn plain_c_does_not_quit() {
        let mut model = Model::default();
        update(&mut model, key(KeyCode::Char('c')));
        assert!(!model.should_quit());
    }

    #[test]
    fn tick_changes_nothing() {
        let mut model = Model::default();
        update(&mut model, Msg::Tick);
        assert_eq!(model.tab(), Tab::Overview);
        assert!(!model.should_quit());
    }
}
