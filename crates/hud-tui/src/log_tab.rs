//! Log tab: filter, search and cursor over the logged actions.

use crossterm::event::{KeyCode, KeyEvent};
use hud_application::{AppEvent, Command, LogCommand, ReportSource};
use hud_domain::action::{ActionRecord, Outcome, Trigger};

use crate::Model;

/// State of the Log tab.
#[derive(Debug, Default)]
pub(crate) struct LogTab {
    /// Index into [`visible`].
    pub(crate) selected: usize,
    /// Search text; matches target, summary and output.
    pub(crate) query: String,
    /// Whether the search input takes the keys.
    pub(crate) searching: bool,
    pub(crate) filter: Filter,
}

/// Which actions the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Filter {
    #[default]
    All,
    Auto,
    Manual,
    /// Everything that did not succeed, and what is still running.
    Problems,
}

impl Filter {
    fn next(self) -> Self {
        match self {
            Self::All => Self::Auto,
            Self::Auto => Self::Manual,
            Self::Manual => Self::Problems,
            Self::Problems => Self::All,
        }
    }

    fn admits(self, record: &ActionRecord) -> bool {
        match self {
            Self::All => true,
            Self::Auto => record.trigger == Trigger::Auto,
            Self::Manual => record.trigger == Trigger::Manual,
            Self::Problems => record.outcome != Some(Outcome::Succeeded),
        }
    }
}

/// Actions that pass filter and search, newest first.
pub(crate) fn visible(model: &Model) -> Vec<&ActionRecord> {
    let tab = &model.log_tab;
    let query = tab.query.to_lowercase();
    model
        .actions
        .iter()
        .filter(|record| tab.filter.admits(record))
        .filter(|record| query.is_empty() || haystack(record).contains(&query))
        .collect()
}

fn haystack(record: &ActionRecord) -> String {
    let detail = match &record.outcome {
        Some(Outcome::Failed(text) | Outcome::Skipped(text)) => text.as_str(),
        _ => "",
    };
    format!(
        "{} {} {detail}",
        record.target,
        record.summary.as_deref().unwrap_or_default()
    )
    .to_lowercase()
}

/// The selected action, if any.
pub(crate) fn selected(model: &Model) -> Option<&ActionRecord> {
    visible(model).get(model.log_tab.selected).copied()
}

/// Handles a key on the Log tab; `None` leaves it to the global keys.
pub(crate) fn on_key(model: &mut Model, key: KeyEvent) -> Option<Vec<Command>> {
    if model.log_tab.searching {
        on_search_key(model, key);
        return Some(Vec::new());
    }
    let count = visible(model).len();
    let tab = &mut model.log_tab;
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            tab.selected = (tab.selected + 1).min(count.saturating_sub(1));
        }
        KeyCode::Up | KeyCode::Char('k') => tab.selected = tab.selected.saturating_sub(1),
        KeyCode::Char('/') => tab.searching = true,
        KeyCode::Char('f') => {
            tab.filter = tab.filter.next();
            tab.selected = 0;
        }
        KeyCode::Esc if !tab.query.is_empty() => {
            tab.query.clear();
            tab.selected = 0;
        }
        KeyCode::Char('r') => return Some(vec![load()]),
        _ => return None,
    }
    Some(Vec::new())
}

/// The search filters while typing; Enter keeps the text, Esc drops it.
fn on_search_key(model: &mut Model, key: KeyEvent) {
    let tab = &mut model.log_tab;
    match key.code {
        KeyCode::Char(c) => tab.query.push(c),
        KeyCode::Backspace => {
            tab.query.pop();
        }
        KeyCode::Enter => tab.searching = false,
        KeyCode::Esc => {
            tab.query.clear();
            tab.searching = false;
        }
        _ => {}
    }
    tab.selected = 0;
}

pub(crate) fn load() -> Command {
    Command::Log(LogCommand::Load)
}

/// Whether `event` means new entries in the log.
pub(crate) fn adds_entries(event: &AppEvent) -> bool {
    match event {
        AppEvent::Job(job) => job.state.is_finished(),
        AppEvent::RepoUpdated(_)
        | AppEvent::Workspaces {
            source: ReportSource::Sync,
            ..
        } => true,
        _ => false,
    }
}

/// Keeps the cursor inside the list after new entries arrived.
pub(crate) fn clamp(model: &mut Model) {
    let count = visible(model).len();
    model.log_tab.selected = model.log_tab.selected.min(count.saturating_sub(1));
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use hud_domain::action::{ActionId, ActionKind};

    use super::*;
    use crate::{Msg, Tab, update};

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn record(id: i64, target: &str, trigger: Trigger, outcome: Outcome) -> ActionRecord {
        let at = "2026-10-17T12:00:00Z".parse().expect("BUG");
        ActionRecord {
            id: ActionId(id),
            started_at: at,
            finished_at: Some(at),
            kind: ActionKind::Install,
            target: target.to_owned(),
            trigger,
            outcome: Some(outcome),
            summary: None,
        }
    }

    fn model() -> Model {
        let mut model = Model {
            tab: Tab::Log,
            ..Model::default()
        };
        let actions = vec![
            record(3, "Homebrew wget", Trigger::Auto, Outcome::Succeeded),
            record(
                2,
                "Npm wrangler",
                Trigger::Manual,
                Outcome::Failed("EACCES".into()),
            ),
            record(1, "/ws/api", Trigger::Auto, Outcome::Succeeded),
        ];
        update(&mut model, Msg::App(AppEvent::Actions(actions)));
        model
    }

    fn targets(model: &Model) -> Vec<&str> {
        visible(model).iter().map(|r| r.target.as_str()).collect()
    }

    #[test]
    fn entering_the_tab_loads_the_log() {
        let mut model = Model {
            tab: Tab::Ai,
            ..Model::default()
        };
        assert_eq!(update(&mut model, key(KeyCode::Tab)), vec![load()]);
    }

    #[test]
    fn filter_cycles_to_automatic_actions() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('f')));
        assert_eq!(targets(&model), vec!["Homebrew wget", "/ws/api"]);
    }

    #[test]
    fn problems_filter_shows_only_what_did_not_succeed() {
        let mut model = model();
        for _ in 0..3 {
            update(&mut model, key(KeyCode::Char('f')));
        }
        assert_eq!(targets(&model), vec!["Npm wrangler"]);
    }

    #[test]
    fn search_matches_the_output_while_typing() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('/')));
        for c in "eacc".chars() {
            update(&mut model, key(KeyCode::Char(c)));
        }
        assert_eq!(targets(&model), vec!["Npm wrangler"]);
    }

    #[test]
    fn q_types_into_the_search_instead_of_quitting() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('/')));
        update(&mut model, key(KeyCode::Char('q')));
        assert!(!model.should_quit());
        assert_eq!(model.log_tab.query, "q");
    }

    #[test]
    fn escape_clears_the_search() {
        let mut model = model();
        update(&mut model, key(KeyCode::Char('/')));
        update(&mut model, key(KeyCode::Char('x')));
        update(&mut model, key(KeyCode::Esc));
        assert_eq!(targets(&model).len(), 3);
        assert!(!model.log_tab.searching);
    }

    #[test]
    fn a_finished_job_on_the_log_tab_reloads() {
        use hud_application::usecases::updates::{JobId, JobState, JobUpdate};
        use hud_domain::update::InstallTarget;
        let mut model = model();
        let job = JobUpdate {
            id: JobId(1),
            target: InstallTarget::AllSkills,
            state: JobState::Succeeded,
        };
        assert_eq!(
            update(&mut model, Msg::App(AppEvent::Job(job))),
            vec![load()]
        );
    }
}
