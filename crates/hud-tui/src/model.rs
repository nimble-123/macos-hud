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

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use hud_application::usecases::updates::{JobId, JobUpdate, SourceReport};
use hud_application::usecases::workspaces::{RepoReport, RepoState, WorkspaceReport};
use hud_domain::git::RepoPath;
use hud_domain::update::InstallTarget;

use hud_application::settings::Settings;
use hud_domain::action::ActionRecord;

use crate::log_tab::LogTab;
use crate::settings::{System, UiSettings};
use crate::settings_tab::SettingsTab;
use crate::theme::Theme;

/// How long a finished fast-forward stays highlighted before it returns to
/// the quiet normal state (`docs/DESIGN.md` §6).
pub(crate) const OK_HIGHLIGHT: Duration = Duration::from_secs(30);

/// Moves a list cursor one row up (`-1`) or down (`1`), wrapping around at
/// both ends. An empty list keeps the cursor at 0.
pub(crate) fn wrap_step(index: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    (index.cast_signed() + delta)
        .rem_euclid(count.cast_signed())
        .cast_unsigned()
}

/// Complete UI state. Changed only by [`crate::update`].
#[derive(Debug)]
pub struct Model {
    pub(crate) settings: UiSettings,
    pub(crate) theme: Theme,
    pub(crate) tab: Tab,
    pub(crate) quit: Quit,
    /// Time of the last tick; the only clock `update` knows.
    pub(crate) now: Option<Instant>,
    pub(crate) workspaces: Vec<WorkspaceReport>,
    pub(crate) syncing: bool,
    /// Index into [`Model::repo_rows`].
    pub(crate) selected: usize,
    /// Repositories fast-forwarded recently, with the time it happened.
    pub(crate) recent_ok: HashMap<RepoPath, Instant>,
    /// Progress of the running scan, if any.
    pub(crate) scan: Option<ScanProgress>,
    /// Number of ticks so far; drives the spinner.
    pub(crate) ticks: u64,
    /// Last report per update source.
    pub(crate) updates: Vec<SourceReport>,
    /// Installation jobs that are queued or running.
    pub(crate) jobs: BTreeMap<JobId, JobUpdate>,
    /// Installation jobs that ended in this session.
    pub(crate) finished_jobs: BTreeMap<JobId, JobUpdate>,
    /// Index into [`Model::update_rows`].
    pub(crate) update_selected: usize,
    /// Installation waiting for confirmation in the dialog.
    pub(crate) confirm: Option<InstallTarget>,
    /// What the system reported at start-up; used for `auto` preferences.
    pub(crate) system: System,
    /// Settings as last reported; `None` until the settings agent sent them.
    pub(crate) preferences: Option<Settings>,
    /// Cursor, input and dialog of the Settings tab.
    pub(crate) settings_tab: SettingsTab,
    /// Most recent logged actions, newest first.
    pub(crate) actions: Vec<ActionRecord>,
    /// Cursor, search and filter of the Log tab.
    pub(crate) log_tab: LogTab,
    /// Zone for showing times; UTC unless the binary sets the system zone.
    pub(crate) time_zone: jiff::tz::TimeZone,
}

/// Where quitting stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quit {
    /// hud keeps running.
    No,
    /// The dialog asks: installations are running.
    Asking,
    /// The user chose to wait; hud quits once the installations ended.
    Waiting,
    /// The event loop stops.
    Now,
}

/// Progress of a running scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanProgress {
    pub(crate) done: usize,
    pub(crate) total: usize,
    pub(crate) fetch: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self::new(UiSettings::default())
    }
}

impl Model {
    /// A fresh model for the given presentation settings.
    #[must_use]
    pub fn new(settings: UiSettings) -> Self {
        Self {
            theme: Theme::new(settings.appearance, settings.color_depth),
            settings,
            tab: Tab::default(),
            quit: Quit::No,
            now: None,
            workspaces: Vec::new(),
            syncing: false,
            selected: 0,
            recent_ok: HashMap::new(),
            scan: None,
            ticks: 0,
            updates: Vec::new(),
            jobs: BTreeMap::new(),
            finished_jobs: BTreeMap::new(),
            update_selected: 0,
            confirm: None,
            system: System {
                language: settings.language,
                appearance: settings.appearance,
            },
            preferences: None,
            settings_tab: SettingsTab::default(),
            actions: Vec::new(),
            log_tab: LogTab::default(),
            time_zone: jiff::tz::TimeZone::UTC,
        }
    }

    /// Sets the zone in which times are shown.
    #[must_use]
    pub fn with_time_zone(mut self, time_zone: jiff::tz::TimeZone) -> Self {
        self.time_zone = time_zone;
        self
    }

    /// Sets what the system reported, for preferences that are `auto`.
    #[must_use]
    pub fn with_system(mut self, system: System) -> Self {
        self.system = system;
        self
    }

    /// Takes over new settings: presentation changes apply at once.
    pub(crate) fn apply_preferences(&mut self, preferences: Settings) {
        self.settings = UiSettings::resolve(&preferences, self.system, self.settings.color_depth);
        self.theme = Theme::new(self.settings.appearance, self.settings.color_depth);
        self.preferences = Some(preferences);
    }

    /// Actionable rows of the Updates tab in display order.
    pub(crate) fn update_rows(&self) -> Vec<InstallTarget> {
        let mut rows = Vec::new();
        for report in &self.updates {
            let Some(Ok(check)) = &report.check else {
                continue;
            };
            if check.updates_known {
                rows.extend(check.candidates.iter().cloned().map(InstallTarget::Package));
            } else if report.source == hud_domain::update::SourceId::Skills {
                rows.push(InstallTarget::AllSkills);
            }
        }
        rows
    }

    /// The job that last touched `target`: a running one first, else the latest finished one.
    pub(crate) fn job_for(&self, target: &InstallTarget) -> Option<&JobUpdate> {
        self.jobs
            .values()
            .find(|job| job.target == *target)
            .or_else(|| {
                self.finished_jobs
                    .values()
                    .rev()
                    .find(|job| job.target == *target)
            })
    }

    /// All repositories in display order, across workspaces.
    pub(crate) fn repo_rows(&self) -> impl Iterator<Item = &RepoReport> {
        self.workspaces
            .iter()
            .filter_map(|w| w.repos.as_ref())
            .flatten()
    }

    /// The selected repository, if any.
    pub(crate) fn selected_repo(&self) -> Option<&RepoReport> {
        self.repo_rows().nth(self.selected)
    }

    /// Whether a repository was fast-forwarded within [`OK_HIGHLIGHT`].
    pub(crate) fn is_recent_ok(&self, repo: &RepoPath) -> bool {
        match (self.recent_ok.get(repo), self.now) {
            (Some(at), Some(now)) => now.duration_since(*at) < OK_HIGHLIGHT,
            _ => false,
        }
    }

    pub(crate) fn mark_fast_forwards(&mut self, reports: &[RepoReport]) {
        let Some(now) = self.now else { return };
        for report in reports {
            if matches!(report.state, RepoState::FastForwarded(_)) {
                self.recent_ok.insert(report.repo.clone(), now);
            }
        }
    }

    /// Presentation settings.
    #[must_use]
    pub fn settings(&self) -> UiSettings {
        self.settings
    }

    /// The active tab.
    #[must_use]
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Whether the event loop should stop.
    #[must_use]
    pub fn should_quit(&self) -> bool {
        self.quit == Quit::Now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_step_goes_from_last_to_first_and_back() {
        assert_eq!(wrap_step(2, 1, 3), 0);
        assert_eq!(wrap_step(0, -1, 3), 2);
        assert_eq!(wrap_step(1, 1, 3), 2);
    }

    #[test]
    fn wrap_step_on_an_empty_list_stays_at_zero() {
        assert_eq!(wrap_step(0, 1, 0), 0);
        assert_eq!(wrap_step(0, -1, 0), 0);
    }

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
