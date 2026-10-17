//! Mouse input: clicks resolve against the same layout `view` draws, then
//! act like the matching key, so mouse and keyboard can never disagree.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use hud_application::Command;

use crate::model::{Quit, TelemetryFlow};
use crate::view::{Hit, hit};
use crate::{Model, Msg, Tab, update};

/// Handles a mouse event.
pub(crate) fn on_mouse(model: &mut Model, event: MouseEvent) -> Vec<Command> {
    match event.kind {
        MouseEventKind::ScrollUp => press(model, KeyCode::Up),
        MouseEventKind::ScrollDown => press(model, KeyCode::Down),
        MouseEventKind::Down(MouseButton::Left) => click(model, event.column, event.row),
        _ => Vec::new(),
    }
}

fn press(model: &mut Model, code: KeyCode) -> Vec<Command> {
    update(model, Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

/// A dialog decides by keys only, so a stray click cannot confirm anything.
fn dialog_open(model: &Model) -> bool {
    model.confirm.is_some()
        || model.quit == Quit::Asking
        || model.telemetry_flow == TelemetryFlow::Confirming
        || model.settings_tab.remove.is_some()
}

fn click(model: &mut Model, x: u16, y: u16) -> Vec<Command> {
    if model.help {
        model.help = false;
        return Vec::new();
    }
    if dialog_open(model) {
        return Vec::new();
    }
    match hit(model, model.area, x, y) {
        Some(Hit::Tab(tab)) => select_tab(model, tab),
        Some(Hit::Key(id)) => key_code(id).map_or_else(Vec::new, |code| press(model, code)),
        Some(Hit::Period(period)) => {
            model.ai_period = period;
            Vec::new()
        }
        Some(Hit::Row(row)) => click_row(model, row),
        None => Vec::new(),
    }
}

fn select_tab(model: &mut Model, tab: Tab) -> Vec<Command> {
    model.tab = tab;
    if tab == Tab::Log {
        return vec![crate::log_tab::load()];
    }
    Vec::new()
}

/// The first click selects a row, a click on the selected row acts like Enter.
fn click_row(model: &mut Model, row: usize) -> Vec<Command> {
    let selected = match model.tab {
        Tab::Workspaces => &mut model.selected,
        Tab::Updates => &mut model.update_selected,
        Tab::Settings => &mut model.settings_tab.selected,
        Tab::Log => &mut model.log_tab.selected,
        Tab::Overview | Tab::Ai => return Vec::new(),
    };
    if *selected == row {
        return press(model, KeyCode::Enter);
    }
    *selected = row;
    Vec::new()
}

/// The key a footer hint stands for; hints that only explain keys do nothing.
fn key_code(id: &str) -> Option<KeyCode> {
    Some(match id {
        "key-r" => KeyCode::Char('r'),
        "key-tab" => KeyCode::Tab,
        "key-quit" => KeyCode::Char('q'),
        "key-f" => KeyCode::Char('f'),
        "key-u" => KeyCode::Char('u'),
        "key-a" => KeyCode::Char('a'),
        "key-d" => KeyCode::Char('d'),
        "key-t" => KeyCode::Char('t'),
        "key-slash" => KeyCode::Char('/'),
        "key-enter" => KeyCode::Enter,
        "key-esc" => KeyCode::Esc,
        "help-key-help" => KeyCode::Char('?'),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crossterm::event::MouseEvent;
    use hud_domain::usage::Period;
    use ratatui::layout::Rect;

    use super::*;

    fn left_click(x: u16, y: u16) -> Msg {
        Msg::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn model() -> Model {
        let mut model = Model::default();
        update(&mut model, Msg::Resize(115, 40));
        model
    }

    #[test]
    fn clicking_a_tab_label_switches_to_it() {
        let mut model = model();
        // "Overview    Workspaces": the second label starts after 8 + 4 columns.
        update(&mut model, left_click(2 + 12 + 1, 0));
        assert_eq!(model.tab, Tab::Workspaces);
    }

    #[test]
    fn clicking_the_log_tab_loads_the_log() {
        let mut model = model();
        let x = (0..115)
            .find(|&x| hit(&model, model.area, x, 0) == Some(Hit::Tab(Tab::Log)))
            .expect("BUG: log tab is visible");
        assert_eq!(
            update(&mut model, left_click(x, 0)),
            vec![crate::log_tab::load()]
        );
    }

    #[test]
    fn clicking_the_quit_hint_quits() {
        let mut model = model();
        let y = 39;
        let x = (0..115)
            .find(|&x| hit(&model, model.area, x, y) == Some(Hit::Key("key-quit")))
            .expect("BUG: quit hint is visible");
        update(&mut model, left_click(x, y));
        assert!(model.should_quit());
    }

    #[test]
    fn clicking_a_period_selects_it_for_the_whole_tab() {
        let mut model = model();
        model.tab = Tab::Ai;
        let target = (0..40)
            .flat_map(|y| (0..115).map(move |x| (x, y)))
            .find(|&(x, y)| hit(&model, model.area, x, y) == Some(Hit::Period(Period::ThisMonth)))
            .expect("BUG: period bar is visible");
        update(&mut model, left_click(target.0, target.1));
        assert_eq!(model.ai_period, Period::ThisMonth);
    }

    #[test]
    fn the_wheel_moves_the_selection() {
        let mut model = model();
        model.tab = Tab::Updates;
        update(
            &mut model,
            Msg::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 10,
                row: 10,
                modifiers: KeyModifiers::NONE,
            }),
        );
        assert_eq!(
            model.update_selected, 0,
            "no rows, nothing moves, nothing breaks"
        );
    }

    #[test]
    fn clicks_do_nothing_while_a_dialog_asks() {
        let mut model = model();
        model.quit = Quit::Asking;
        update(&mut model, left_click(15, 0));
        assert_eq!(model.tab, Tab::Overview);
    }

    #[test]
    fn clicks_outside_any_target_do_nothing() {
        let mut model = model();
        assert_eq!(update(&mut model, left_click(110, 20)), Vec::new());
        assert_eq!(model.area, Rect::new(0, 0, 115, 40));
    }
}
