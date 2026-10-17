//! Storing telemetry as it arrives, and switching it on in Claude Code.

use hud_domain::usage::TelemetryPoint;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::event::{AppEvent, TelemetryCommand};
use crate::ports::telemetry::{
    ClaudeSettings, ClaudeTelemetry, ReceiverState, TelemetryStore, classify, telemetry_entries,
};

/// Receiver and Claude Code setup, as the AI tab shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryStatus {
    /// Whether hud listens.
    pub receiver: ReceiverState,
    /// Whether Claude Code sends to it.
    pub claude: ClaudeTelemetry,
    /// Entries hud would add, for the confirmation dialog.
    pub entries: Vec<(String, String)>,
}

/// Reports the telemetry setup and adds the entries on request.
pub struct TelemetrySetupAgent<P> {
    settings: P,
    receiver: ReceiverState,
    port: u16,
}

impl<P: ClaudeSettings> TelemetrySetupAgent<P> {
    /// Agent for the receiver on `port` in state `receiver`.
    pub fn new(settings: P, receiver: ReceiverState, port: u16) -> Self {
        Self {
            settings,
            receiver,
            port,
        }
    }

    /// Reports once at start and after every command.
    pub async fn run(
        self,
        mut commands: mpsc::Receiver<TelemetryCommand>,
        events: mpsc::Sender<AppEvent>,
        cancel: CancellationToken,
    ) {
        loop {
            let status = self.status().await;
            if events
                .send(AppEvent::Telemetry(Box::new(status)))
                .await
                .is_err()
            {
                return;
            }
            let command = tokio::select! {
                () = cancel.cancelled() => return,
                command = commands.recv() => command,
            };
            match command {
                None => return,
                Some(TelemetryCommand::Refresh) => {}
                Some(TelemetryCommand::Enable) => self.enable().await,
            }
        }
    }

    async fn status(&self) -> TelemetryStatus {
        let claude = match self.settings.env().await {
            Ok(env) => classify(&env, self.port),
            Err(reason) => ClaudeTelemetry::Unreadable(reason),
        };
        TelemetryStatus {
            receiver: self.receiver.clone(),
            claude,
            entries: telemetry_entries(self.port)
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        }
    }

    /// Adds only missing entries, and only when nothing conflicts; the
    /// file is read again right before writing.
    async fn enable(&self) {
        let env = match self.settings.env().await {
            Ok(env) => env,
            Err(reason) => {
                tracing::warn!(%reason, "claude settings not readable");
                return;
            }
        };
        if classify(&env, self.port) != ClaudeTelemetry::Disabled {
            return;
        }
        let missing: Vec<_> = telemetry_entries(self.port)
            .into_iter()
            .filter(|(key, _)| !env.iter().any(|(k, _)| k == key))
            .collect();
        if let Err(reason) = self.settings.add_env(missing).await {
            tracing::warn!(%reason, "could not enable claude telemetry");
        }
    }
}

/// Stores batches of telemetry points sent by the receiver.
pub struct TelemetryAgent<S> {
    store: S,
}

impl<S: TelemetryStore> TelemetryAgent<S> {
    /// Agent over the store.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Stores every batch until cancelled or the receiver is gone.
    pub async fn run(
        self,
        mut points: mpsc::Receiver<Vec<TelemetryPoint>>,
        cancel: CancellationToken,
    ) {
        loop {
            let batch = tokio::select! {
                () = cancel.cancelled() => return,
                batch = points.recv() => batch,
            };
            let Some(batch) = batch else { return };
            if let Err(error) = self.store.save_points(batch).await {
                tracing::warn!(%error, "could not store telemetry");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakeSettings {
        env: Mutex<Vec<(String, String)>>,
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the fake answers immediately; the trait still needs async signatures"
    )]
    impl ClaudeSettings for FakeSettings {
        async fn env(&self) -> Result<Vec<(String, String)>, String> {
            Ok(self.env.lock().expect("BUG").clone())
        }

        async fn add_env(&self, entries: Vec<(&'static str, String)>) -> Result<(), String> {
            self.env
                .lock()
                .expect("BUG")
                .extend(entries.into_iter().map(|(k, v)| (k.to_owned(), v)));
            Ok(())
        }
    }

    async fn next_status(events: &mut mpsc::Receiver<AppEvent>) -> TelemetryStatus {
        match events.recv().await {
            Some(AppEvent::Telemetry(status)) => *status,
            other => panic!("expected telemetry status, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn enable_adds_the_entries_and_reports_enabled() {
        let agent = TelemetrySetupAgent::new(
            FakeSettings::default(),
            ReceiverState::Listening(4318),
            4318,
        );
        let (commands, commands_rx) = mpsc::channel(2);
        let (events_tx, mut events) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands_rx, events_tx, cancel.clone()));
        assert_eq!(
            next_status(&mut events).await.claude,
            ClaudeTelemetry::Disabled
        );
        commands.send(TelemetryCommand::Enable).await.expect("BUG");
        assert_eq!(
            next_status(&mut events).await.claude,
            ClaudeTelemetry::Enabled
        );
        cancel.cancel();
        task.await.expect("BUG");
    }

    #[tokio::test]
    async fn enable_leaves_a_conflicting_setup_alone() {
        let settings = FakeSettings::default();
        settings.env.lock().expect("BUG").push((
            "OTEL_EXPORTER_OTLP_ENDPOINT".into(),
            "https://otel.example.com".into(),
        ));
        let agent = TelemetrySetupAgent::new(settings, ReceiverState::Listening(4318), 4318);
        let (commands, commands_rx) = mpsc::channel(2);
        let (events_tx, mut events) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(agent.run(commands_rx, events_tx, cancel.clone()));
        let _ = next_status(&mut events).await;
        commands.send(TelemetryCommand::Enable).await.expect("BUG");
        let status = next_status(&mut events).await;
        assert!(matches!(status.claude, ClaudeTelemetry::Conflict(_)));
        cancel.cancel();
        task.await.expect("BUG");
    }
}
