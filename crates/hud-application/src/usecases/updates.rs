//! Checking update sources and running installations as jobs.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use hud_domain::action::{ActionKind, Outcome};
use hud_domain::update::{
    AutoInstallPolicy, DenyReason, InstallDecision, InstallRequest, InstallTarget, SourceId,
    install_decision,
};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::event::{AppEvent, UpdateCommand};
use crate::ports::store::{ActionLog, Clock};
use crate::ports::updates::{Availability, InstallOutcome, SourceCheck, UpdateError, UpdateSource};

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

struct Shared<L, C> {
    sources: Vec<Arc<dyn UpdateSource>>,
    log: L,
    clock: C,
    /// One lock per source: never two brew or two npm processes at once.
    locks: HashMap<SourceId, Arc<Mutex<()>>>,
    events: mpsc::Sender<AppEvent>,
}

/// Checks sources on start, on a timer and on request, and runs installations.
pub struct UpdatesAgent<L, C> {
    shared: Arc<Shared<L, C>>,
    interval: Duration,
}

impl<L, C> UpdatesAgent<L, C>
where
    L: ActionLog + 'static,
    C: Clock + 'static,
{
    /// Agent over the given sources; `interval` is the pause between checks.
    pub fn new(
        sources: Vec<Arc<dyn UpdateSource>>,
        log: L,
        clock: C,
        interval: Duration,
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
                clock,
                locks,
                events,
            }),
            interval,
        }
    }

    /// Runs until cancelled. Installation jobs run in their own tasks so a
    /// long `brew upgrade` never delays checks or other sources.
    pub async fn run(self, mut commands: mpsc::Receiver<UpdateCommand>, cancel: CancellationToken) {
        let shared = &self.shared;
        // Checks run as tasks as well: `brew update` can take minutes and must
        // not delay commands or cancellation.
        let mut jobs = JoinSet::new();
        jobs.spawn(Arc::clone(shared).check_and_report());
        let mut timer = tokio::time::interval_at(Instant::now() + self.interval, self.interval);
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut next_id = 0u64;
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                _ = timer.tick() => {
                    jobs.spawn(Arc::clone(shared).check_and_report());
                }
                Some(_) = jobs.join_next(), if !jobs.is_empty() => {}
                command = commands.recv() => match command {
                    None => break,
                    Some(UpdateCommand::CheckNow) => {
                        jobs.spawn(Arc::clone(shared).check_and_report());
                    }
                    Some(UpdateCommand::Install { target, confirmed }) => {
                        next_id += 1;
                        let shared = Arc::clone(shared);
                        jobs.spawn(async move { shared.install(JobId(next_id), target, confirmed).await });
                    }
                },
            }
        }
        // Dropping the tasks stops their processes (kill on drop); the quit
        // dialog (#26) decides beforehand whether to wait for them.
        jobs.shutdown().await;
    }
}

impl<L: ActionLog, C: Clock> Shared<L, C> {
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

    async fn check_and_report(self: Arc<Self>) {
        let reports = self.check_all().await;
        let _ = self.send(AppEvent::Updates(reports)).await;
    }

    /// Checks under the source's lock, so a check never runs next to an
    /// installation by the same package manager.
    async fn check_locked(&self, source: &dyn UpdateSource) -> SourceReport {
        let lock = self.locks.get(&source.id()).cloned().unwrap_or_default();
        let _guard = lock.lock().await;
        check_source(source).await
    }

    async fn install(&self, id: JobId, target: InstallTarget, confirmed: bool) {
        let source_id = target_source(&target);
        let report = |state| {
            AppEvent::Job(JobUpdate {
                id,
                target: target.clone(),
                state,
            })
        };
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
        let _ = self.send(report(JobState::Running)).await;

        // Manual only: automatic installation is the scheduler's job (#23).
        let request = InstallRequest::Manual { confirmed };
        let permit =
            match install_decision(target.clone(), request, AutoInstallPolicy::default(), false) {
                InstallDecision::Allowed(permit) => permit,
                InstallDecision::Denied(reason) => {
                    let _ = self.send(report(JobState::Denied(reason))).await;
                    return;
                }
            };
        let label = describe(&target);
        let action = self
            .log
            .start(
                self.clock.now(),
                ActionKind::Install,
                &label,
                permit.trigger(),
            )
            .await;
        if let Err(error) = &action {
            tracing::warn!(%error, "could not log installation start");
        }
        let state = match source.install(permit).await {
            Ok(InstallOutcome::Installed) => JobState::Succeeded,
            Ok(InstallOutcome::NeedsInteraction) => JobState::NeedsInteraction,
            Err(UpdateError::TimedOut(_)) => JobState::TimedOut,
            Err(error) => JobState::Failed(error.to_string()),
        };
        if let Ok(action) = action {
            let outcome = match &state {
                JobState::Succeeded => Outcome::Succeeded,
                JobState::NeedsInteraction => Outcome::Skipped("needs a password".into()),
                JobState::TimedOut => Outcome::TimedOut,
                JobState::Failed(message) => Outcome::Failed(message.clone()),
                JobState::Queued | JobState::Running | JobState::Denied(_) => {
                    Outcome::Skipped(String::new())
                }
            };
            if let Err(error) = self
                .log
                .finish(action, self.clock.now(), outcome, None)
                .await
            {
                tracing::warn!(%error, "could not log installation result");
            }
        }
        let _ = self.send(report(state)).await;
        let _ = self
            .send(AppEvent::SourceUpdated(check_source(source.as_ref()).await))
            .await;
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::future::BoxFuture;
    use hud_domain::update::{InstallPermit, PackageKind, PackageName, UpdateCandidate, Version};

    use super::*;
    use crate::testing::{FakeClock, FakeLog};

    struct FakeSource {
        id: SourceId,
        availability: Availability,
        candidates: Vec<UpdateCandidate>,
        installs: AtomicUsize,
        running: AtomicUsize,
        max_parallel: AtomicUsize,
        delay: Duration,
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
                tokio::time::sleep(self.delay).await;
                self.running.fetch_sub(1, Ordering::SeqCst);
                self.installs.fetch_add(1, Ordering::SeqCst);
                Ok(InstallOutcome::Installed)
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

    fn start(
        sources: Vec<Arc<dyn UpdateSource>>,
    ) -> (
        mpsc::Sender<UpdateCommand>,
        mpsc::Receiver<AppEvent>,
        CancellationToken,
        tokio::task::JoinHandle<()>,
        Arc<FakeLog>,
    ) {
        let (events_tx, events) = mpsc::channel(64);
        let (commands, commands_rx) = mpsc::channel(8);
        let log = Arc::new(FakeLog::default());
        let agent = UpdatesAgent::new(
            sources,
            Arc::clone(&log),
            FakeClock,
            Duration::from_hours(6),
            events_tx,
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands_rx, cancel.clone()));
        (commands, events, cancel, task, log)
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
        let (_commands, mut events, cancel, task, _log) = start(vec![brew, Arc::new(npm)]);
        let AppEvent::Updates(reports) = events.recv().await.expect("BUG") else {
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
        cancel.cancel();
        task.await.expect("BUG");
    }

    #[tokio::test]
    async fn confirmed_install_runs_logs_and_rechecks_the_source() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let (commands, mut events, cancel, task, log) =
            start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = events.recv().await;
        let target = InstallTarget::Package(candidate(SourceId::Homebrew, "wget"));
        commands
            .send(UpdateCommand::Install {
                target,
                confirmed: true,
            })
            .await
            .expect("BUG");

        let states = job_states(&mut events, 1).await;
        assert_eq!(
            states,
            vec![JobState::Queued, JobState::Running, JobState::Succeeded]
        );
        assert!(matches!(
            events.recv().await,
            Some(AppEvent::SourceUpdated(_))
        ));
        assert_eq!(brew.installs.load(Ordering::SeqCst), 1);
        let records = log.records.lock().expect("BUG").clone();
        assert_eq!(records[0].kind, ActionKind::Install);
        assert_eq!(records[0].outcome, Some(Outcome::Succeeded));
        cancel.cancel();
        task.await.expect("BUG");
    }

    #[tokio::test]
    async fn unconfirmed_install_is_denied_without_touching_the_tool() {
        let brew = Arc::new(FakeSource::new(SourceId::Homebrew));
        let (commands, mut events, cancel, task, _log) =
            start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = events.recv().await;
        let target = InstallTarget::Package(candidate(SourceId::Homebrew, "wget"));
        commands
            .send(UpdateCommand::Install {
                target,
                confirmed: false,
            })
            .await
            .expect("BUG");

        let states = job_states(&mut events, 1).await;
        assert_eq!(
            states.last(),
            Some(&JobState::Denied(DenyReason::NotConfirmed))
        );
        assert_eq!(brew.installs.load(Ordering::SeqCst), 0);
        cancel.cancel();
        task.await.expect("BUG");
    }

    #[tokio::test]
    async fn installs_of_the_same_source_never_run_in_parallel() {
        let mut slow = FakeSource::new(SourceId::Homebrew);
        slow.delay = Duration::from_millis(30);
        let brew = Arc::new(slow);
        let (commands, mut events, cancel, task, _log) =
            start(vec![brew.clone() as Arc<dyn UpdateSource>]);
        let _ = events.recv().await;
        for name in ["a", "b", "c"] {
            let target = InstallTarget::Package(candidate(SourceId::Homebrew, name));
            commands
                .send(UpdateCommand::Install {
                    target,
                    confirmed: true,
                })
                .await
                .expect("BUG");
        }
        let _ = job_states(&mut events, 3).await;
        assert_eq!(brew.max_parallel.load(Ordering::SeqCst), 1);
        assert_eq!(brew.installs.load(Ordering::SeqCst), 3);
        cancel.cancel();
        task.await.expect("BUG");
    }

    #[tokio::test]
    async fn check_now_reports_all_sources_again() {
        let (commands, mut events, cancel, task, _log) =
            start(vec![Arc::new(FakeSource::new(SourceId::Npm))]);
        let _ = events.recv().await;
        commands.send(UpdateCommand::CheckNow).await.expect("BUG");
        assert!(matches!(events.recv().await, Some(AppEvent::Updates(_))));
        cancel.cancel();
        task.await.expect("BUG");
    }

    #[test]
    fn skills_target_belongs_to_the_skills_source() {
        assert_eq!(target_source(&InstallTarget::AllSkills), SourceId::Skills);
    }
}
