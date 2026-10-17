//! Settings tab: rows, cursor, path input and the remove dialog.
//!
//! Changes are never applied locally: the tab sends a command and shows the
//! settings the agent reports back, so the screen always matches the file.

use crossterm::event::{KeyCode, KeyEvent};
use hud_application::settings::{
    AutoInstallKind, IconPreference, LanguagePreference, Settings, SettingsChange, ThemePreference,
};
use hud_application::{Command, SettingsCommand};
use hud_domain::WorkspacePath;

use crate::Model;
use crate::model::wrap_step;

/// State of the Settings tab.
#[derive(Debug, Default)]
pub(crate) struct SettingsTab {
    /// Index into [`rows`].
    pub(crate) selected: usize,
    /// Path being typed for a new workspace; `Some` while the input is open.
    pub(crate) input: Option<String>,
    /// Workspace waiting for confirmation of its removal.
    pub(crate) remove: Option<WorkspacePath>,
    /// Message about the last action.
    pub(crate) notice: Option<Notice>,
}

/// Message below the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    /// The typed path is neither absolute nor `~/`-relative.
    InvalidPath,
    /// The agent rejected a change or reload.
    Rejected(String),
}

/// One selectable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    Language(LanguagePreference),
    Theme(ThemePreference),
    Icons(IconPreference),
    Workspace {
        path: WorkspacePath,
        auto_fast_forward: bool,
    },
    AddWorkspace,
    AutoInstall {
        kind: AutoInstallKind,
        enabled: bool,
    },
}

/// Rows in display order.
pub(crate) fn rows(settings: &Settings) -> Vec<Row> {
    let mut rows = vec![
        Row::Language(settings.language),
        Row::Theme(settings.theme),
        Row::Icons(settings.icons),
    ];
    rows.extend(settings.workspaces.iter().map(|workspace| Row::Workspace {
        path: workspace.path.clone(),
        auto_fast_forward: workspace.auto_fast_forward,
    }));
    rows.push(Row::AddWorkspace);
    let opt_ins = settings.auto_install;
    rows.extend(
        [
            (AutoInstallKind::BrewFormulae, opt_ins.brew_formulae),
            (AutoInstallKind::BrewCasks, opt_ins.brew_casks),
            (AutoInstallKind::Npm, opt_ins.npm),
        ]
        .map(|(kind, enabled)| Row::AutoInstall { kind, enabled }),
    );
    rows
}

/// Handles a key on the Settings tab; `None` leaves it to the global keys.
pub(crate) fn on_key(model: &mut Model, key: KeyEvent) -> Option<Vec<Command>> {
    if model.settings_tab.input.is_some() {
        return Some(on_input_key(model, key));
    }
    if model.settings_tab.remove.is_some() {
        return Some(on_remove_key(model, key));
    }
    let preferences = model.preferences.as_ref()?;
    let rows = rows(preferences);
    let tab = &mut model.settings_tab;
    let row = rows.get(tab.selected).cloned();
    let change = match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            tab.selected = wrap_step(tab.selected, 1, rows.len());
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            tab.selected = wrap_step(tab.selected, -1, rows.len());
            None
        }
        KeyCode::Char('r') => {
            tab.notice = None;
            return Some(vec![Command::Settings(SettingsCommand::Reload)]);
        }
        KeyCode::Char('a') => {
            open_input(tab);
            None
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(Row::Workspace { path, .. }) = row {
                tab.remove = Some(path);
            }
            None
        }
        KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => match row {
            Some(Row::AddWorkspace) => {
                open_input(tab);
                None
            }
            Some(row) => change_for(&row, 1),
            None => None,
        },
        KeyCode::Left => row.and_then(|row| change_for(&row, -1)),
        _ => return None,
    };
    Some(change.map(command).into_iter().collect())
}

fn open_input(tab: &mut SettingsTab) {
    tab.input = Some(String::new());
    tab.notice = None;
}

fn command(change: SettingsChange) -> Command {
    Command::Settings(SettingsCommand::Change(change))
}

/// The change a row makes: choices cycle by `step`, switches toggle.
fn change_for(row: &Row, step: isize) -> Option<SettingsChange> {
    Some(match row {
        Row::Language(current) => SettingsChange::Language(cycle(
            &[
                LanguagePreference::Auto,
                LanguagePreference::De,
                LanguagePreference::En,
            ],
            *current,
            step,
        )),
        Row::Theme(current) => SettingsChange::Theme(cycle(
            &[
                ThemePreference::Auto,
                ThemePreference::Dark,
                ThemePreference::Light,
            ],
            *current,
            step,
        )),
        Row::Icons(current) => SettingsChange::Icons(cycle(
            &[
                IconPreference::Unicode,
                IconPreference::Nerd,
                IconPreference::Ascii,
            ],
            *current,
            step,
        )),
        Row::Workspace {
            path,
            auto_fast_forward,
        } => SettingsChange::AutoFastForward {
            workspace: path.clone(),
            enabled: !auto_fast_forward,
        },
        Row::AutoInstall { kind, enabled } => SettingsChange::AutoInstall {
            kind: *kind,
            enabled: !enabled,
        },
        Row::AddWorkspace => return None,
    })
}

fn cycle<T: Copy + PartialEq>(values: &[T], current: T, step: isize) -> T {
    let position = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let count = values.len().cast_signed();
    let next = (position.cast_signed() + step).rem_euclid(count);
    values[next.cast_unsigned()]
}

/// The input takes all keys: typing, Backspace, Enter to add, Esc to close.
fn on_input_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    let tab = &mut model.settings_tab;
    let Some(input) = tab.input.as_mut() else {
        return Vec::new();
    };
    match key.code {
        KeyCode::Char(c) => {
            input.push(c);
            tab.notice = None;
        }
        KeyCode::Backspace => {
            input.pop();
        }
        KeyCode::Esc => {
            tab.input = None;
            tab.notice = None;
        }
        KeyCode::Enter => {
            let path = input.trim().trim_end_matches('/').to_owned();
            if !(path.starts_with('/') || path.starts_with("~/")) {
                tab.notice = Some(Notice::InvalidPath);
                return Vec::new();
            }
            tab.input = None;
            return vec![command(SettingsChange::AddWorkspace(path))];
        }
        _ => {}
    }
    Vec::new()
}

/// The dialog takes all keys: Enter removes, Esc or `n` closes.
fn on_remove_key(model: &mut Model, key: KeyEvent) -> Vec<Command> {
    let tab = &mut model.settings_tab;
    match key.code {
        KeyCode::Enter => tab
            .remove
            .take()
            .map(|path| vec![command(SettingsChange::RemoveWorkspace(path))])
            .unwrap_or_default(),
        KeyCode::Esc | KeyCode::Char('n') => {
            tab.remove = None;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Keeps the cursor inside the rows after the settings changed.
pub(crate) fn clamp(model: &mut Model) {
    let count = model.preferences.as_ref().map_or(0, |p| rows(p).len());
    model.settings_tab.selected = model.settings_tab.selected.min(count.saturating_sub(1));
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::KeyModifiers;
    use hud_application::AppEvent;
    use hud_application::settings::WorkspaceSettings;

    use super::*;
    use crate::{Msg, Tab, update};

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn workspace(path: &str) -> WorkspacePath {
        WorkspacePath::try_from(PathBuf::from(path)).expect("BUG: absolute")
    }

    fn model() -> Model {
        let mut model = Model {
            tab: Tab::Settings,
            ..Model::default()
        };
        let settings = Settings {
            workspaces: vec![WorkspaceSettings {
                path: workspace("/ws"),
                auto_fast_forward: true,
            }],
            ..Settings::default()
        };
        update(&mut model, Msg::App(AppEvent::Settings(Box::new(settings))));
        model
    }

    fn select(model: &mut Model, wanted: &Row) {
        let rows = rows(model.preferences.as_ref().expect("BUG: loaded"));
        model.settings_tab.selected = rows.iter().position(|r| r == wanted).expect("BUG: row");
    }

    #[test]
    fn selection_wraps_at_both_ends() {
        let mut model = model();
        let last = rows(model.preferences.as_ref().expect("BUG: loaded")).len() - 1;
        update(&mut model, key(KeyCode::Up));
        assert_eq!(model.settings_tab.selected, last, "up from the first row");
        update(&mut model, key(KeyCode::Down));
        assert_eq!(model.settings_tab.selected, 0, "down from the last row");
    }

    #[test]
    fn enter_cycles_the_language_forward_and_left_backward() {
        let mut model = model();
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![command(SettingsChange::Language(LanguagePreference::De))]
        );
        assert_eq!(
            update(&mut model, key(KeyCode::Left)),
            vec![command(SettingsChange::Language(LanguagePreference::En))]
        );
    }

    #[test]
    fn space_toggles_fast_forward_of_the_selected_workspace() {
        let mut model = model();
        select(
            &mut model,
            &Row::Workspace {
                path: workspace("/ws"),
                auto_fast_forward: true,
            },
        );
        assert_eq!(
            update(&mut model, key(KeyCode::Char(' '))),
            vec![command(SettingsChange::AutoFastForward {
                workspace: workspace("/ws"),
                enabled: false,
            })]
        );
    }

    #[test]
    fn opt_ins_toggle() {
        let mut model = model();
        select(
            &mut model,
            &Row::AutoInstall {
                kind: AutoInstallKind::Npm,
                enabled: false,
            },
        );
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![command(SettingsChange::AutoInstall {
                kind: AutoInstallKind::Npm,
                enabled: true,
            })]
        );
    }

    #[test]
    fn typed_path_is_added_on_enter() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('a')));
        for c in "~/code/".chars() {
            update(&mut model, key(KeyCode::Char(c)));
        }
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![command(SettingsChange::AddWorkspace("~/code".into()))]
        );
        assert_eq!(model.settings_tab.input, None);
    }

    #[test]
    fn relative_path_is_rejected_and_the_input_stays_open() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('a')));
        update(&mut model, key(KeyCode::Char('x')));
        assert_eq!(update(&mut model, key(KeyCode::Enter)), Vec::new());
        assert_eq!(model.settings_tab.notice, Some(Notice::InvalidPath));
        assert!(model.settings_tab.input.is_some());
    }

    #[test]
    fn q_types_into_the_input_instead_of_quitting() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('a')));
        update(&mut model, key(KeyCode::Char('q')));
        assert!(!model.should_quit());
        assert_eq!(model.settings_tab.input.as_deref(), Some("q"));
    }

    #[test]
    fn removing_a_workspace_needs_confirmation() {
        let mut model = model();
        select(
            &mut model,
            &Row::Workspace {
                path: workspace("/ws"),
                auto_fast_forward: true,
            },
        );
        assert_eq!(update(&mut model, key(KeyCode::Char('d'))), Vec::new());
        assert_eq!(
            update(&mut model, key(KeyCode::Enter)),
            vec![command(SettingsChange::RemoveWorkspace(workspace("/ws")))]
        );
    }

    #[test]
    fn escape_cancels_the_removal() {
        let mut model = model();
        select(
            &mut model,
            &Row::Workspace {
                path: workspace("/ws"),
                auto_fast_forward: true,
            },
        );
        update(&mut model, key(KeyCode::Char('d')));
        assert_eq!(update(&mut model, key(KeyCode::Esc)), Vec::new());
        assert_eq!(model.settings_tab.remove, None);
    }

    #[test]
    fn r_reloads_the_config() {
        let mut model = model();
        assert_eq!(
            update(&mut model, key(KeyCode::Char('r'))),
            vec![Command::Settings(SettingsCommand::Reload)]
        );
    }

    #[test]
    fn new_settings_apply_to_the_presentation_at_once() {
        let mut model = model();
        let settings = Settings {
            language: LanguagePreference::De,
            icons: IconPreference::Ascii,
            ..Settings::default()
        };
        update(&mut model, Msg::App(AppEvent::Settings(Box::new(settings))));
        assert_eq!(model.settings.language, crate::Language::De);
        assert_eq!(model.settings.icons, crate::IconSet::Ascii);
    }

    #[test]
    fn a_rejected_change_is_shown() {
        let mut model = model();
        update(
            &mut model,
            Msg::App(AppEvent::SettingsRejected("bad".into())),
        );
        assert_eq!(
            model.settings_tab.notice,
            Some(Notice::Rejected("bad".into()))
        );
    }
}
