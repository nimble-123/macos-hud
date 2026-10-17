//! Changing and reloading the settings while hud runs.

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::event::{AppEvent, SettingsCommand};
use crate::settings::{Settings, SettingsStore};

/// Applies settings changes and publishes the result.
///
/// The other agents watch the published settings and pick up changes on
/// their own, so no agent needs to know another.
pub struct SettingsAgent<S> {
    store: S,
    current: watch::Sender<Settings>,
}

impl<S: SettingsStore> SettingsAgent<S> {
    /// Agent over `store`; `current` holds the settings hud started with.
    pub fn new(store: S, current: watch::Sender<Settings>) -> Self {
        Self { store, current }
    }

    /// Sends the current settings, then handles commands until cancelled.
    pub async fn run(
        self,
        mut commands: mpsc::Receiver<SettingsCommand>,
        events: mpsc::Sender<AppEvent>,
        cancel: CancellationToken,
    ) {
        let initial = self.current.borrow().clone();
        if events
            .send(AppEvent::Settings(Box::new(initial)))
            .await
            .is_err()
        {
            return;
        }
        loop {
            let command = tokio::select! {
                () = cancel.cancelled() => return,
                command = commands.recv() => command,
            };
            let Some(command) = command else { return };
            let result = match command {
                SettingsCommand::Change(change) => self.store.update(&change).await,
                SettingsCommand::Reload => self.store.load().await,
            };
            let event = match result {
                Ok(settings) => {
                    // No receivers is fine: then nothing needs to react.
                    self.current.send_replace(settings.clone());
                    AppEvent::Settings(Box::new(settings))
                }
                Err(error) => {
                    tracing::warn!(%error, "settings not changed");
                    AppEvent::SettingsRejected(error.to_string())
                }
            };
            if events.send(event).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::settings::{
        InitialSetup, LanguagePreference, SettingsChange, SettingsError, ThemePreference,
    };

    /// Applies language changes; rejects everything else as invalid.
    #[derive(Default)]
    struct FakeStore {
        file: Mutex<Settings>,
    }

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the fake answers immediately; the trait still needs async signatures"
    )]
    impl SettingsStore for FakeStore {
        async fn load(&self) -> Result<Settings, SettingsError> {
            Ok(self.file.lock().expect("BUG").clone())
        }

        async fn apply_setup(&self, _setup: &InitialSetup) -> Result<Settings, SettingsError> {
            unreachable!("not used by the agent")
        }

        async fn update(&self, change: &SettingsChange) -> Result<Settings, SettingsError> {
            let mut file = self.file.lock().expect("BUG");
            match change {
                SettingsChange::Language(language) => file.language = *language,
                _ => {
                    return Err(SettingsError::Invalid {
                        path: "config.toml".into(),
                        message: "unsupported".into(),
                    });
                }
            }
            Ok(file.clone())
        }
    }

    struct Running {
        commands: mpsc::Sender<SettingsCommand>,
        events: mpsc::Receiver<AppEvent>,
        watched: watch::Receiver<Settings>,
        store: std::sync::Arc<FakeStore>,
        cancel: CancellationToken,
        task: tokio::task::JoinHandle<()>,
    }

    impl SettingsStore for std::sync::Arc<FakeStore> {
        async fn load(&self) -> Result<Settings, SettingsError> {
            (**self).load().await
        }
        async fn apply_setup(&self, setup: &InitialSetup) -> Result<Settings, SettingsError> {
            (**self).apply_setup(setup).await
        }
        async fn update(&self, change: &SettingsChange) -> Result<Settings, SettingsError> {
            (**self).update(change).await
        }
    }

    fn start() -> Running {
        let store = std::sync::Arc::new(FakeStore::default());
        let (current, watched) = watch::channel(Settings::default());
        let (commands, commands_rx) = mpsc::channel(4);
        let (events_tx, events) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let agent = SettingsAgent::new(std::sync::Arc::clone(&store), current);
        let task = tokio::spawn(agent.run(commands_rx, events_tx, cancel.clone()));
        Running {
            commands,
            events,
            watched,
            store,
            cancel,
            task,
        }
    }

    impl Running {
        async fn send_and_receive(&mut self, command: SettingsCommand) -> AppEvent {
            self.commands.send(command).await.expect("BUG: agent alive");
            self.events.recv().await.expect("BUG: agent alive")
        }

        async fn stop(self) {
            self.cancel.cancel();
            self.task.await.expect("BUG: agent task");
        }
    }

    #[tokio::test]
    async fn start_sends_the_current_settings() {
        let mut agent = start();
        assert!(matches!(
            agent.events.recv().await,
            Some(AppEvent::Settings(settings)) if *settings == Settings::default()
        ));
        agent.stop().await;
    }

    #[tokio::test]
    async fn a_change_is_reported_and_published_to_watchers() {
        let mut agent = start();
        let _ = agent.events.recv().await;
        let event = agent
            .send_and_receive(SettingsCommand::Change(SettingsChange::Language(
                LanguagePreference::De,
            )))
            .await;
        assert!(matches!(event, AppEvent::Settings(s) if s.language == LanguagePreference::De));
        assert!(agent.watched.has_changed().expect("BUG: sender alive"));
        assert_eq!(agent.watched.borrow().language, LanguagePreference::De);
        agent.stop().await;
    }

    #[tokio::test]
    async fn a_rejected_change_keeps_the_published_settings() {
        let mut agent = start();
        let _ = agent.events.recv().await;
        let event = agent
            .send_and_receive(SettingsCommand::Change(SettingsChange::Theme(
                ThemePreference::Light,
            )))
            .await;
        assert!(matches!(event, AppEvent::SettingsRejected(_)));
        assert!(!agent.watched.has_changed().expect("BUG: sender alive"));
        agent.stop().await;
    }

    #[tokio::test]
    async fn reload_reads_the_file_again() {
        let mut agent = start();
        let _ = agent.events.recv().await;
        agent.store.file.lock().expect("BUG").language = LanguagePreference::En;
        let event = agent.send_and_receive(SettingsCommand::Reload).await;
        assert!(matches!(event, AppEvent::Settings(s) if s.language == LanguagePreference::En));
        agent.stop().await;
    }
}
