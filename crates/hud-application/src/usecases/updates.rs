//! Checking update sources and running installations as jobs.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use hud_domain::action::{ActionKind, Outcome};
use hud_domain::update::{
    AutoInstallPolicy, DenyReason, InstallDecision, InstallPermit, InstallRequest, InstallTarget,
    SourceId, UpdateCandidate, install_decision,
};
use tokio::sync::{Mutex, mpsc, watch};
use tokio::task::{AbortHandle, JoinSet};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::event::{AppEvent, UpdateCommand};
use crate::ports::store::{ActionLog, Clock, InstallHistory, InstallResult};
use crate::ports::updates::{Availability, InstallOutcome, SourceCheck, UpdateError, UpdateSource};
use crate::settings::Settings;

/// State of one source after a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceReport {
    /// Which source.
    pub source: SourceId,
    /// Whether its tool can be used.
    pub availability: Availability,
    /// Result of the check; `None` when the tool is not available.
    pub check: Option<Result<SourceCheck, UpdateError>>,
}

/// Identifies an installation job within one run of hud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(pub u64);

/// Lifecycle of an installation job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Waiting for the source's lock.
    Queued,
    /// The tool is running.
    Running,
    /// Installed.
    Succeeded,
    /// The tool asked for a password; nothing was installed.
    NeedsInteraction,
    /// The rules did not allow it.
    Denied(DenyReason),
    /// The tool failed.
    Failed(String),
    /// The tool took too long and was stopped.
    TimedOut,
    /// hud quit before or while it ran.
    Cancelled,
}

impl JobState {
    /// Whether the job is over.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

/// A job changed state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobUpdate {
    /// Job.
    pub id: JobId,
    /// What it installs.
    pub target: InstallTarget,
    /// New state.
    pub state: JobState,
}

/// Source of an installation target.
#[must_use]
pub fn target_source(target: &InstallTarget) -> SourceId {
    match target {
        InstallTarget::Package(candidate) => candidate.source,
        InstallTarget::AllSkills => SourceId::Skills,
    }
}

struct Shared<L, H, C> {
    sources: Vec<Arc<dyn UpdateSource>>,
    log: L,
    history: H,
    clock: C,
    /// Opt-ins and the check interval; read on use, so changes apply at once.
    settings: watch::Receiver<Settings>,
    /// One lock per source: never two brew or two npm processes at once.
    locks: HashMap<SourceId, Arc<Mutex<()>>>,
    events: mpsc::Sender<AppEvent>,
    next_job: AtomicU64,
    /// Set while automatic installations run, so a second check does not
    /// queue the same packages again.
    auto_running: AtomicBool,
    /// Set once the agent stops: nothing new starts from then on.
    stopping: AtomicBool,
}

/// Checks sources on start, on a timer and on request, and runs installations.
pub struct UpdatesAgent<L, H, C> {
    shared: Arc<Shared<L, H, C>>,
}

impl<L, H, C> UpdatesAgent<L, H, C>
where
    L: ActionLog + 'static,
    H: InstallHistory + 'static,
    C: Clock + 'static,
{
    /// Agent over the given sources; opt-ins and the check interval come
    /// from `settings` and follow its changes.
    pub fn new(
        sources: Vec<Arc<dyn UpdateSource>>,
        log: L,
        history: H,
        clock: C,
        settings: watch::Receiver<Settings>,
        events: mpsc::Sender<AppEvent>,
    ) -> Self {
        let locks = sources
            .iter()
            .map(|source| (source.id(), Arc::new(Mutex::new(()))))
            .collect();
        Self {
            shared: Arc::new(Shared {
                sources,
                log,
                history,
                clock,
                settings,
                locks,
                events,
                next_job: AtomicU64::new(1),
                auto_running: AtomicBool::new(false),
                stopping: AtomicBool::new(false),
            }),
        }
    }

    /// Runs until cancelled or drained. Installation jobs run in their own
    /// tasks so a long `brew upgrade` never delays checks or other sources.
    ///
    /// On the way out it always waits for the running tasks: after a drain
    /// they finish normally, after a cancel the process runner stops their
    /// tools (SIGTERM, then SIGKILL) and they are logged as cancelled.
    pub async fn run(self, mut commands: mpsc::Receiver<UpdateCommand>, cancel: CancellationToken) {
        let shared = &self.shared;
        // Checks run as tasks as well: `brew update` can take minutes and must
        // not delay commands or cancellation.
        let mut jobs = JoinSet::new();
        let mut checks = Vec::new();
        spawn_check(shared, &mut jobs, &mut checks);
        let mut changes = shared.settings.clone();
        let mut interval = changes.borrow_and_update().update_check_interval;
        let mut timer = check_timer(interval);
        let mut drained = false;
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                _ = timer.tick() => spawn_check(shared, &mut jobs, &mut checks),
                Ok(()) = changes.changed() => {
                    // New opt-ins are read on the next check; only the timer needs work.
                    let period = changes.borrow_and_update().update_check_interval;
                    if period != interval {
                        interval = period;
                        timer = check_timer(interval);
                    }
                }
                Some(_) = jobs.join_next(), if !jobs.is_empty() => {}
                command = commands.recv() => match command {
                    None => break,
                    Some(UpdateCommand::CheckNow) => spawn_check(shared, &mut jobs, &mut checks),
                    Some(UpdateCommand::Drain) => {
                        drained = true;
                        break;
                    }
                    Some(UpdateCommand::Install { target, confirmed }) => {
                        let shared = Arc::clone(shared);
                        jobs.spawn(async move {
                            shared.install(target, InstallRequest::Manual { confirmed }).await;
                        });
                    }
                },
            }
        }
        shared.stopping.store(true, Ordering::Release);
        if drained {
            // Checks change nothing on the system; waiting for them is pointless.
            for check in &checks {
                check.abort();
            }
        }
        while jobs.join_next().await.is_some() {}
        if drained {
            let _ = shared.send(AppEvent::JobsFinished).await;
        }
    }
}

fn spawn_check<L, H, C>(
    shared: &Arc<Shared<L, H, C>>,
    jobs: &mut JoinSet<()>,
    checks: &mut Vec<AbortHandle>,
) where
    L: ActionLog + 'static,
    H: InstallHistory + 'static,
    C: Clock + 'static,
{
    checks.retain(|check| !check.is_finished());
    checks.push(jobs.spawn(Arc::clone(shared).check_and_report()));
}

fn check_timer(period: Duration) -> tokio::time::Interval {
    let mut timer = tokio::time::interval_at(Instant::now() + period, period);
    timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
    timer
}

impl<L: ActionLog, H: InstallHistory, C: Clock> Shared<L, H, C> {
    fn policy(&self) -> AutoInstallPolicy {
        self.settings.borrow().auto_install.into()
    }

    async fn send(&self, event: AppEvent) -> Result<(), ()> {
        self.events.send(event).await.map_err(|_| ())
    }

    async fn check_all(&self) -> Vec<SourceReport> {
        futures::future::join_all(
            self.sources
                .iter()
                .map(|source| self.check_locked(source.as_ref())),
        )
        .await
    }

    /// Reports every source, then installs what the opt-ins allow.
    async fn check_and_report(self: Arc<Self>) {
        let reports = self.check_all().await;
        let candidates: Vec<UpdateCandidate> = reports
            .iter()
            .filter_map(|report| report.check.as_ref()?.as_ref().ok())
            .flat_map(|check| check.candidates.iter().cloned())
            .collect();
        if self.send(AppEvent::Updates(reports)).await.is_ok() {
            self.auto_install(candidates).await;
        }
    }

    /// Installs every candidate the rules allow, one after another. Denied
    /// candidates stay silent: not installing is the normal case.
    async fn auto_install(&self, candidates: Vec<UpdateCandidate>) {
        if self.policy() == AutoInstallPolicy::default()
            || self.auto_running.swap(true, Ordering::AcqRel)
        {
            return;
        }
        for candidate in candidates {
            if self.stopping.load(Ordering::Acquire) {
                break;
            }
            let target = InstallTarget::Package(candidate);
            if let InstallDecision::Allowed(permit) =
                self.decide(target, InstallRequest::Auto).await
            {
                self.run_permit(permit).await;
            }
        }
        self.auto_running.store(false, Ordering::Release);
    }

    /// Checks under the source's lock, so a check never runs next to an
    /// installation by the same package manager.
    async fn check_locked(&self, source: &dyn UpdateSource) -> SourceReport {
        let lock = self.locks.get(&source.id()).cloned().unwrap_or_default();
        let _guard = lock.lock().await;
        check_source(source).await
    }

    async fn decide(&self, target: InstallTarget, request: InstallRequest) -> InstallDecision {
        let needed_before = match &target {
            InstallTarget::Package(candidate) if request == InstallRequest::Auto => self
                .history
                .needed_interaction(candidate.source, &candidate.name)
                .await
                // Without history, assume the worst: skipping beats a prompt.
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "could not read install history");
                    true
                }),
            _ => false,
        };
        install_decision(target, request, self.policy(), needed_before)
    }

    async fn install(&self, target: InstallTarget, request: InstallRequest) {
        match self.decide(target.clone(), request).await {
            InstallDecision::Allowed(permit) => self.run_permit(permit).await,
            InstallDecision::Denied(reason) => {
                let update = JobUpdate {
                    id: self.job_id(),
                    target,
                    state: JobState::Denied(reason),
                };
                let _ = self.send(AppEvent::Job(update)).await;
            }
        }
    }

    fn job_id(&self) -> JobId {
        JobId(self.next_job.fetch_add(1, Ordering::Relaxed))
    }

    async fn run_permit(&self, permit: InstallPermit) {
        let id = self.job_id();
        let target = permit.target().clone();
        let report = |state| {
            AppEvent::Job(JobUpdate {
                id,
                target: target.clone(),
                state,
            })
        };
        let source_id = target_source(&target);
        let Some(source) = self
            .sources
            .iter()
            .find(|source| source.id() == source_id)
            .cloned()
        else {
            let _ = self
                .send(report(JobState::Failed(format!(
                    "source {source_id:?} not configured"
                ))))
                .await;
            return;
        };
        let _ = self.send(report(JobState::Queued)).await;
        let lock = self.locks.get(&source_id).cloned().unwrap_or_default();
        let _guard = lock.lock().await;
        if self.stopping.load(Ordering::Acquire) {
            // Queued, never started: nothing to log.
            let _ = self.send(report(JobState::Cancelled)).await;
            return;
        }
        let _ = self.send(report(JobState::Running)).await;

        let action = self
            .log
            .start(
                self.clock.now(),
                ActionKind::Install,
                &describe(&target),
                permit.trigger(),
            )
            .await;
        if let Err(error) = &action {
            tracing::warn!(%error, "could not log installation start");
        }
        let mut summary = None;
        let state = match source.install(permit).await {
            Ok(InstallOutcome::Installed { output }) => {
                summary = Some(output).filter(|output| !output.is_empty());
                JobState::Succeeded
            }
            Ok(InstallOutcome::NeedsInteraction) => JobState::NeedsInteraction,
            Err(UpdateError::TimedOut(_)) => JobState::TimedOut,
            Err(UpdateError::Cancelled(_)) => JobState::Cancelled,
            Err(error) => JobState::Failed(error.to_string()),
        };
        if let Ok(action) = action
            && let Err(error) = self
                .log
                .finish(action, self.clock.now(), log_outcome(&state), summary)
                .await
        {
            tracing::warn!(%error, "could not log installation result");
        }
        self.remember(&target, &state).await;
        let _ = self.send(report(state)).await;
        let _ = self
            .send(AppEvent::SourceUpdated(check_source(source.as_ref()).await))
            .await;
    }

    async fn remember(&self, target: &InstallTarget, state: &JobState) {
        let InstallTarget::Package(candidate) = target else {
            return;
        };
        let result = match state {
            JobState::Succeeded => InstallResult::Installed,
            JobState::NeedsInteraction => InstallResult::NeedsInteraction,
            // A cancelled run says nothing about the package.
            JobState::Cancelled => return,
            _ => InstallResult::Failed,
        };
        if let Err(error) = self
            .history
            .record(self.clock.now(), candidate, result)
            .await
        {
            tracing::warn!(%error, "could not record installation result");
        }
    }
}

fn log_outcome(state: &JobState) -> Outcome {
    match state {
        JobState::Succeeded => Outcome::Succeeded,
        JobState::NeedsInteraction => Outcome::Skipped("needs a password".into()),
        JobState::TimedOut => Outcome::TimedOut,
        JobState::Cancelled => Outcome::Cancelled,
        JobState::Failed(message) => Outcome::Failed(message.clone()),
        JobState::Queued | JobState::Running | JobState::Denied(_) => {
            Outcome::Skipped(String::new())
        }
    }
}

async fn check_source(source: &dyn UpdateSource) -> SourceReport {
    let availability = source.availability().await;
    let check = match availability {
        Availability::Available(_) => Some(source.check().await),
        Availability::Missing | Availability::Broken(_) => None,
    };
    SourceReport {
        source: source.id(),
        availability,
        check,
    }
}

/// Short text for logs: what is installed, from which version to which.
fn describe(target: &InstallTarget) -> String {
    match target {
        InstallTarget::Package(candidate) => {
            format!(
                "{:?} {} {} → {}",
                candidate.source, candidate.name, candidate.current, candidate.latest
            )
        }
        InstallTarget::AllSkills => "Skills: update all".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use futures::future::BoxFuture;
    use hud_domain::action::Trigger;
    use hud_domain::update::{PackageKind, PackageName, Version};

    use super::*;
    use crate::testing::{FakeClock, FakeHistory, FakeLog};

    struct FakeSource {
        id: SourceId,
        availability: Availability,
        candidates: Vec<UpdateCandidate>,
        installs: AtomicUsize,
        running: AtomicUsize,
        max_parallel: AtomicUsize,
        delay: Duration,
        outcome: InstallOutcome,
        /// Behaves like the process runner: a cancel stops the tool.
        cancel: CancellationToken,
    }

    impl FakeSource {
        fn new(id: SourceId) -> Self {
            Self {
                id,
                availability: Availability::Available("1.0".into()),
                candidates: vec![candidate(id, "wget")],
                installs: AtomicUsize::new(0),
                running: AtomicUsize::new(0),
                max_parallel: AtomicUsize::new(0),
                delay: Duration::ZERO,
                outcome: InstallOutcome::Installed {
                    output: "🍺 wget 1.1".into(),
                },
                cancel: CancellationToken::new(),
            }
        }
    }

    impl UpdateSource for FakeSource {
        fn id(&self) -> SourceId {
            self.id
        }
        fn availability(&self) -> BoxFuture<'_, Availability> {
            Box::pin(async move { self.availability.clone() })
        }
        fn check(&self) -> BoxFuture<'_, Result<SourceCheck, UpdateError>> {
            Box::pin(async move {
                Ok(SourceCheck {
                    candidates: self.candidates.clone(),
                    installed: Vec::new(),
                    updates_known: true,
                })
            })
        }
        fn install(
            &self,
            _permit: InstallPermit,
        ) -> BoxFuture<'_, Result<InstallOutcome, UpdateError>> {
            Box::pin(async move {
                let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_parallel.fetch_max(now, Ordering::SeqCst);
                let cancelled = tokio::select! {
                    () = tokio::time::sleep(self.delay) => false,
                    () = self.cancel.cancelled() => true,
                };
                self.running.fetch_sub(1, Ordering::SeqCst);
                if cancelled {
                    return Err(UpdateError::Cancelled("fake"));
                }
                self.installs.fetch_add(1, Ordering::SeqCst);
                Ok(self.outcome.clone())
            })
        }
    }

    fn candidate(source: SourceId, name: &str) -> UpdateCandidate {
        UpdateCandidate {
            source,
            kind: PackageKind::Formula,
            name: PackageName::try_from(name.to_owned()).expect("BUG"),
            current: Version::new("1.0"),
            latest: Version::new("1.1"),
            pinned: false,
        }
    }

    struct Running {
        commands: mpsc::Sender<UpdateCommand>,
        events: mpsc::Receiver<AppEvent>,
        cancel: CancellationToken,
        task: tokio::task::JoinHandle<()>,
        log: Arc<FakeLog>,
        history: Arc<FakeHistory>,
        settings: watch::Sender<Settings>,
    }

    impl Running {
        async fn stop(self) {
            self.cancel.cancel();
            self.task.await.expect("BUG: agent task");
        }
    }

    fn start(sources: Vec<Arc<dyn UpdateSource>>) -> Running {
        start_with(
            sources,
            AutoInstallPolicy::default(),
            FakeHistory::default(),
        )
    }

    fn start_with(
        sources: Vec<Arc<dyn UpdateSource>>,
        policy: AutoInstallPolicy,
        history: FakeHistory,
    ) -> Running {
        let (events_tx, events) = mpsc::channel(64);
        let (commands, commands_rx) = mpsc::channel(8);
        let log = Arc::new(FakeLog::default());
        let history = Arc::new(history);
        let (settings, watched) = watch::channel(Settings {
            auto_install: crate::settings::AutoInstallSettings {
                brew_formulae: policy.brew_formulae,
                brew_casks: policy.brew_casks,
                npm: policy.npm,
            },
            ..Settings::default()
        });
        let agent = UpdatesAgent::new(
            sources,
            Arc::clone(&log),
            Arc::clone(&history),
            FakeClock,
            watched,
            events_tx,
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands_rx, cancel.clone()));
        Running {
            commands,
            events,
            cancel,
            task,
            log,
            history,
            settings,
        }
    }

    async fn job_states(
        events: &mut mpsc::Receiver<AppEvent>,
        until_finished: usize,
    ) -> Vec<JobState> {
        let mut states = Vec::new();
        let mut finished = 0;
        while finished < until_finished {
            if let AppEvent::Job(update) = events.recv().await.expect("BUG: agent alive") {
                finished += usize::from(update.state.is_finished());
                states.push(update.state);
            }
        }
        states
    }

    #[tokio::test]
    async fn start_reports_every_source_and_skips_checks_of_missing_tools() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let mut npm = FakeSource::new(SourceId::Npm);
        npm.availability = Availability::Missing;
        let mut agent = start(vec![brew, Arc::new(npm)]);
        let AppEvent::Updates(reports) = agent.events.recv().await.expect("BUG") else {
            panic!("expected updates")
        };
        assert_eq!(reports.len(), 2);
        assert_eq!(
            reports[0]
                .check
                .as_ref()
                .expect("BUG")
                .as_ref()
                .expect("BUG")
                .candidates
                .len(),
            1
        );
        assert_eq!(reports[1].check, None);
        agent.stop().await;
    }

    #[tokio::test]
    async fn confirmed_install_runs_logs_and_rechecks_the_source() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let mut agent = start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = agent.events.recv().await;
        let target = InstallTarget::Package(candidate(SourceId::Homebrew, "wget"));
        agent
            .commands
            .send(UpdateCommand::Install {
                target,
                confirmed: true,
            })
            .await
            .expect("BUG");

        let states = job_states(&mut agent.events, 1).await;
        assert_eq!(
            states,
            vec![JobState::Queued, JobState::Running, JobState::Succeeded]
        );
        assert!(matches!(
            agent.events.recv().await,
            Some(AppEvent::SourceUpdated(_))
        ));
        assert_eq!(brew.installs.load(Ordering::SeqCst), 1);
        let records = agent.log.records.lock().expect("BUG").clone();
        assert_eq!(records[0].kind, ActionKind::Install);
        assert_eq!(records[0].outcome, Some(Outcome::Succeeded));
        assert_eq!(records[0].summary.as_deref(), Some("🍺 wget 1.1"));
        agent.stop().await;
    }

    #[tokio::test]
    async fn unconfirmed_install_is_denied_without_touching_the_tool() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let mut agent = start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = agent.events.recv().await;
        let target = InstallTarget::Package(candidate(SourceId::Homebrew, "wget"));
        agent
            .commands
            .send(UpdateCommand::Install {
                target,
                confirmed: false,
            })
            .await
            .expect("BUG");

        let states = job_states(&mut agent.events, 1).await;
        assert_eq!(
            states.last(),
            Some(&JobState::Denied(DenyReason::NotConfirmed))
        );
        assert_eq!(brew.installs.load(Ordering::SeqCst), 0);
        agent.stop().await;
    }

    #[tokio::test]
    async fn installs_of_the_same_source_never_run_in_parallel() {
        let mut slow = FakeSource::new(SourceId::Homebrew);
        slow.delay = Duration::from_millis(30);
        let brew = Arc::new(slow);
        let mut agent = start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = agent.events.recv().await;
        for name in ["a", "b", "c"] {
            let target = InstallTarget::Package(candidate(SourceId::Homebrew, name));
            agent
                .commands
                .send(UpdateCommand::Install {
                    target,
                    confirmed: true,
                })
                .await
                .expect("BUG");
        }
        let _ = job_states(&mut agent.events, 3).await;
        assert_eq!(brew.max_parallel.load(Ordering::SeqCst), 1);
        assert_eq!(brew.installs.load(Ordering::SeqCst), 3);
        agent.stop().await;
    }

    fn slow_brew() -> FakeSource {
        let mut source = FakeSource::new(SourceId::Homebrew);
        source.delay = Duration::from_millis(200);
        source
    }

    async fn install(agent: &Running, name: &str) {
        let target = InstallTarget::Package(candidate(SourceId::Homebrew, name));
        agent
            .commands
            .send(UpdateCommand::Install {
                target,
                confirmed: true,
            })
            .await
            .expect("BUG");
    }

    /// Waits until a job reports `Running`.
    async fn until_running(events: &mut mpsc::Receiver<AppEvent>) {
        loop {
            if let Some(AppEvent::Job(job)) = events.recv().await
                && job.state == JobState::Running
            {
                return;
            }
        }
    }

    #[tokio::test]
    async fn drain_lets_the_running_install_finish_and_cancels_queued_ones() {
        let brew = Arc::new(slow_brew());
        let mut agent = start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = agent.events.recv().await;
        install(&agent, "a").await;
        install(&agent, "b").await;
        until_running(&mut agent.events).await;
        agent
            .commands
            .send(UpdateCommand::Drain)
            .await
            .expect("BUG");

        let mut finished = Vec::new();
        loop {
            match agent.events.recv().await.expect("BUG: agent alive") {
                AppEvent::Job(job) if job.state.is_finished() => finished.push(job.state),
                AppEvent::JobsFinished => break,
                _ => {}
            }
        }
        finished.sort_by_key(|state| format!("{state:?}"));
        assert_eq!(finished, vec![JobState::Cancelled, JobState::Succeeded]);
        assert_eq!(brew.installs.load(Ordering::SeqCst), 1);
        agent.task.await.expect("BUG: agent ends after the drain");
    }

    #[tokio::test]
    async fn cancel_waits_for_the_stopped_install_and_logs_it_as_cancelled() {
        let mut source = slow_brew();
        source.delay = Duration::from_secs(60);
        let brew = Arc::new(source);
        let mut agent = start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = agent.events.recv().await;
        install(&agent, "a").await;
        until_running(&mut agent.events).await;

        // In hud the same token reaches the process runner.
        brew.cancel.cancel();
        agent.cancel.cancel();
        agent.task.await.expect("BUG: agent task");
        let records = agent.log.records.lock().expect("BUG").clone();
        assert_eq!(records[0].outcome, Some(Outcome::Cancelled));
        assert!(agent.history.entries.lock().expect("BUG").is_empty());
    }

    #[tokio::test]
    async fn check_now_reports_all_sources_again() {
        let mut agent = start(vec![Arc::new(FakeSource::new(SourceId::Npm))]);
        let _ = agent.events.recv().await;
        agent
            .commands
            .send(UpdateCommand::CheckNow)
            .await
            .expect("BUG");
        assert!(matches!(
            agent.events.recv().await,
            Some(AppEvent::Updates(_))
        ));
        agent.stop().await;
    }

    fn cask(name: &str) -> UpdateCandidate {
        UpdateCandidate {
            kind: PackageKind::Cask,
            ..candidate(SourceId::Homebrew, name)
        }
    }

    fn casks_only() -> AutoInstallPolicy {
        AutoInstallPolicy {
            brew_casks: true,
            ..AutoInstallPolicy::default()
        }
    }

    #[tokio::test]
    async fn opted_in_candidates_are_installed_after_the_check() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let policy = AutoInstallPolicy {
            brew_formulae: true,
            ..AutoInstallPolicy::default()
        };
        let mut agent = start_with(
            vec![brew.clone() as Arc<dyn UpdateSource>],
            policy,
            FakeHistory::default(),
        );
        assert!(matches!(
            agent.events.recv().await,
            Some(AppEvent::Updates(_))
        ));

        let states = job_states(&mut agent.events, 1).await;
        assert_eq!(states.last(), Some(&JobState::Succeeded));
        assert_eq!(brew.installs.load(Ordering::SeqCst), 1);
        let records = agent.log.records.lock().expect("BUG").clone();
        assert_eq!(records[0].trigger, Trigger::Auto);
        agent.stop().await;
    }

    #[tokio::test]
    async fn nothing_is_installed_without_an_opt_in() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let mut agent = start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = agent.events.recv().await;
        agent
            .commands
            .send(UpdateCommand::CheckNow)
            .await
            .expect("BUG");
        // The next event is the second check, not a job.
        assert!(matches!(
            agent.events.recv().await,
            Some(AppEvent::Updates(_))
        ));
        assert_eq!(brew.installs.load(Ordering::SeqCst), 0);
        agent.stop().await;
    }

    #[tokio::test]
    async fn an_opt_in_enabled_while_running_applies_to_the_next_check() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let mut agent = start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = agent.events.recv().await;
        agent
            .settings
            .send_modify(|settings| settings.auto_install.brew_formulae = true);
        agent
            .commands
            .send(UpdateCommand::CheckNow)
            .await
            .expect("BUG");
        let _ = agent.events.recv().await;
        let states = job_states(&mut agent.events, 1).await;
        assert_eq!(states.last(), Some(&JobState::Succeeded));
        agent.stop().await;
    }

    #[tokio::test]
    async fn a_cask_that_asked_for_a_password_is_remembered() {
        let mut source = FakeSource::new(SourceId::Homebrew);
        source.candidates = vec![cask("firefox")];
        source.outcome = InstallOutcome::NeedsInteraction;
        let brew = Arc::new(source);
        let mut agent = start_with(
            vec![brew.clone() as Arc<dyn UpdateSource>],
            casks_only(),
            FakeHistory::default(),
        );
        let _ = agent.events.recv().await;

        let states = job_states(&mut agent.events, 1).await;
        assert_eq!(states.last(), Some(&JobState::NeedsInteraction));
        let entries = agent.history.entries.lock().expect("BUG").clone();
        assert_eq!(entries[0].2, InstallResult::NeedsInteraction);
        agent.stop().await;
    }

    #[tokio::test]
    async fn a_cask_that_asked_for_a_password_is_not_retried_automatically() {
        let mut source = FakeSource::new(SourceId::Homebrew);
        source.candidates = vec![cask("firefox")];
        let brew = Arc::new(source);
        let history = FakeHistory::default();
        history.entries.lock().expect("BUG").push((
            SourceId::Homebrew,
            cask("firefox").name,
            InstallResult::NeedsInteraction,
        ));
        let mut agent = start_with(
            vec![brew.clone() as Arc<dyn UpdateSource>],
            casks_only(),
            history,
        );
        let _ = agent.events.recv().await;
        agent
            .commands
            .send(UpdateCommand::CheckNow)
            .await
            .expect("BUG");
        assert!(matches!(
            agent.events.recv().await,
            Some(AppEvent::Updates(_))
        ));
        assert_eq!(brew.installs.load(Ordering::SeqCst), 0);
        agent.stop().await;
    }

    #[test]
    fn skills_target_belongs_to_the_skills_source() {
        assert_eq!(target_source(&InstallTarget::AllSkills), SourceId::Skills);
    }
}
