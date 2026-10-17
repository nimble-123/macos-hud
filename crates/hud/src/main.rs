//! hud binary: composition root, terminal lifecycle and event loop.

mod environment;
mod logging;
mod terminal;

use std::time::Duration;

use anyhow::Context;
use crossterm::event::EventStream;
use futures::StreamExt;
use std::path::PathBuf;

use hud_application::settings::{Settings, SettingsStore};
use hud_application::usecases::agent::{WorkspaceAgent, WorkspaceConfig};
use hud_application::{AppEvent, Command};
use hud_infrastructure::clock::SystemClock;
use hud_infrastructure::config::TomlSettings;
use hud_infrastructure::discovery::FsDiscovery;
use hud_infrastructure::git::GitCli;
use hud_infrastructure::process::TokioProcessRunner;
use hud_infrastructure::sqlite::SqliteStore;
use hud_tui::{Model, Msg, update, view};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

/// Capacity of the channel from use cases to the UI (bounded for backpressure).
const APP_EVENT_CAPACITY: usize = 256;
/// Capacity of the channel from the UI to the use cases.
const COMMAND_CAPACITY: usize = 64;
/// Upper bound for waiting on background work when quitting.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const RENDER_TICK: Duration = Duration::from_millis(250);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _log_guard = logging::init().context("failed to initialise logging")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting hud");

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

    // Composition root: the only place where adapters meet use cases.
    let store = SqliteStore::open(&database_path()?, jiff::Timestamp::now())
        .context("failed to open the database")?;
    let git_program = preferences
        .tools
        .git
        .as_ref()
        .map_or_else(|| "git".to_owned(), |path| path.display().to_string());
    let agent = WorkspaceAgent::new(
        GitCli::new(runner, git_program),
        FsDiscovery,
        store.clone(),
        store,
        SystemClock,
        WorkspaceConfig {
            workspaces: preferences.workspaces.clone(),
            scan: preferences.scan.clone(),
            fetch_interval: preferences.fetch_interval,
        },
    );
    let (app_events_tx, mut app_events) = mpsc::channel::<AppEvent>(APP_EVENT_CAPACITY);
    let (commands, commands_rx) = mpsc::channel::<Command>(COMMAND_CAPACITY);
    let cancel = CancellationToken::new();
    let background = tokio::spawn(agent.run(commands_rx, app_events_tx, cancel.clone()));

    let settings = environment::ui_settings(&preferences);
    let mut session = terminal::Session::enter().context("failed to prepare the terminal")?;
    let result = run(
        session.terminal(),
        Model::new(settings),
        &mut app_events,
        &commands,
    )
    .await;
    drop(session);

    cancel.cancel();
    drop(commands);
    if tokio::time::timeout(SHUTDOWN_GRACE, background)
        .await
        .is_err()
    {
        tracing::warn!("background work still running at exit");
    }
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

/// `~/Library/Application Support/hud/hud.db`.
fn database_path() -> anyhow::Result<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    Ok(home.join("Library/Application Support/hud/hud.db"))
}

async fn run(
    terminal: &mut terminal::Tui,
    mut model: Model,
    app_events: &mut mpsc::Receiver<AppEvent>,
    commands: &mpsc::Sender<Command>,
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
            _ = render_tick.tick() => Msg::Tick(std::time::Instant::now()),
        };

        for command in update(&mut model, msg) {
            commands.send(command).await?;
        }
        if model.should_quit() {
            return Ok(());
        }
    }
}
