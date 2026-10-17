//! hud binary: composition root, terminal lifecycle and event loop.

mod environment;
mod logging;
mod terminal;

use std::time::Duration;

use anyhow::Context;
use crossterm::event::EventStream;
use futures::StreamExt;
use std::path::PathBuf;

use hud_application::AppEvent;
use hud_application::settings::{Settings, SettingsStore};
use hud_infrastructure::config::TomlSettings;
use hud_infrastructure::process::TokioProcessRunner;
use hud_tui::{Model, Msg, update, view};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

/// Capacity of the channel from use cases to the UI (bounded for backpressure).
const APP_EVENT_CAPACITY: usize = 256;
const RENDER_TICK: Duration = Duration::from_millis(250);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _log_guard = logging::init().context("failed to initialise logging")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting hud");

    // The sender is handed to use cases once they exist; keeping it alive keeps
    // the receiving branch of the event loop active.
    let (_app_events_tx, mut app_events) = mpsc::channel::<AppEvent>(APP_EVENT_CAPACITY);

    let preferences = load_settings().await?;
    // Resolving the login shell PATH can take seconds; start it now and let
    // the first external process wait for it instead of the UI.
    let runner = TokioProcessRunner::with_login_path(
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned()),
        std::env::var_os("PATH").unwrap_or_default(),
    );
    tokio::spawn({
        let runner = runner.clone();
        async move {
            let _ = runner.path().await;
        }
    });
    let settings = environment::ui_settings(&preferences);
    let mut session = terminal::Session::enter().context("failed to prepare the terminal")?;
    let result = run(session.terminal(), Model::new(settings), &mut app_events).await;
    drop(session);

    tracing::info!("stopping hud");
    result
}

/// Invalid settings stop hud before the terminal is taken over, so the error
/// stays readable in the shell.
async fn load_settings() -> anyhow::Result<Settings> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let store = TomlSettings::new(TomlSettings::default_path(&home, xdg.as_deref()), home);
    let settings = store.load().await?;
    tracing::info!(
        path = %store.path().display(),
        workspaces = settings.workspaces.len(),
        needs_setup = settings.needs_setup(),
        "loaded settings"
    );
    Ok(settings)
}

async fn run(
    terminal: &mut terminal::Tui,
    mut model: Model,
    app_events: &mut mpsc::Receiver<AppEvent>,
) -> anyhow::Result<()> {
    let mut input = EventStream::new();
    let mut render_tick = tokio::time::interval(RENDER_TICK);
    render_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        terminal.draw(|frame| view(&model, frame))?;

        // All three branches are cancel-safe: EventStream::next, Receiver::recv
        // and Interval::tick lose nothing when another branch wins.
        let msg = tokio::select! {
            Some(event) = input.next() => match Msg::from_terminal(&event?) {
                Some(msg) => msg,
                None => continue,
            },
            Some(event) = app_events.recv() => Msg::App(event),
            _ = render_tick.tick() => Msg::Tick,
        };

        #[expect(
            clippy::never_loop,
            reason = "Command has no variants until the first use case exists"
        )]
        for command in update(&mut model, msg) {
            match command {}
        }
        if model.should_quit() {
            return Ok(());
        }
    }
}
