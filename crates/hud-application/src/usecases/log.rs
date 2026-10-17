//! Reading the action log for the Log tab.

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::event::{AppEvent, LogCommand};
use crate::ports::store::ActionLog;

/// How many actions the Log tab shows; older ones stay in the database
/// until the retention period ends.
pub const RECENT_ACTIONS: u32 = 500;

/// Answers requests for the most recent actions.
pub struct LogAgent<L> {
    log: L,
}

impl<L: ActionLog> LogAgent<L> {
    /// Agent over the action log.
    pub fn new(log: L) -> Self {
        Self { log }
    }

    /// Handles commands until cancelled or the UI is gone.
    pub async fn run(
        self,
        mut commands: mpsc::Receiver<LogCommand>,
        events: mpsc::Sender<AppEvent>,
        cancel: CancellationToken,
    ) {
        loop {
            let command = tokio::select! {
                () = cancel.cancelled() => return,
                command = commands.recv() => command,
            };
            let Some(LogCommand::Load) = command else {
                return;
            };
            let records = match self.log.recent(RECENT_ACTIONS).await {
                Ok(records) => records,
                Err(error) => {
                    // The tab keeps what it showed; the next request tries again.
                    tracing::warn!(%error, "could not read the action log");
                    continue;
                }
            };
            if events.send(AppEvent::Actions(records)).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hud_domain::action::{ActionKind, Trigger};

    use super::*;
    use crate::ports::store::Clock;
    use crate::testing::{FakeClock, FakeLog};

    #[tokio::test]
    async fn load_sends_the_logged_actions() {
        let log = Arc::new(FakeLog::default());
        log.start(
            FakeClock.now(),
            ActionKind::FastForward,
            "/ws/api",
            Trigger::Auto,
        )
        .await
        .expect("BUG");
        let (commands, commands_rx) = mpsc::channel(4);
        let (events_tx, mut events) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(LogAgent::new(log).run(commands_rx, events_tx, cancel.clone()));

        commands.send(LogCommand::Load).await.expect("BUG");
        let Some(AppEvent::Actions(records)) = events.recv().await else {
            panic!("expected actions")
        };
        assert_eq!(records[0].target, "/ws/api");
        cancel.cancel();
        task.await.expect("BUG");
    }
}
