//! hud binary: composition root, terminal lifecycle and event loop.

mod cli;
mod environment;
mod logging;
mod terminal;

use std::time::Duration;

use anyhow::Context;
use crossterm::event::EventStream;
use futures::StreamExt;
use std::path::PathBuf;

use hud_application::settings::{IconPreference, InitialSetup, Settings, SettingsStore};
use hud_application::usecases::agent::{WorkspaceAgent, WorkspaceConfig};
use hud_application::{AppEvent, Command};
use hud_infrastructure::clock::SystemClock;
use hud_infrastructure::config::TomlSettings;
use hud_infrastructure::discovery::FsDiscovery;
use hud_infrastructure::git::GitCli;
use hud_infrastructure::process::TokioProcessRunner;
use hud_infrastructure::sqlite::SqliteStore;
use hud_tui::{
    IconSet, Model, Msg, SetupModel, SetupOutcome, update, update_setup, view, view_setup,
};
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
async fn main() -> anyhow::Result<std::process::ExitCode> {
    match cli::parse(std::env::args().skip(1)) {
        cli::Action::Run => {}
        cli::Action::Print(text) => {
            println!("{text}");
            return Ok(std::process::ExitCode::SUCCESS);
        }
        cli::Action::Fail(text) => {
            eprintln!("{text}");
            return Ok(std::process::ExitCode::from(2));
        }
    }

    let _log_guard = logging::init().context("failed to initialise logging")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting hud");

    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let config = config_store(&home);
    let mut preferences = load_settings(&config).await?;

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

    let mut settings = environment::ui_settings(&preferences);
    let mut session = terminal::Session::enter().context("failed to prepare the terminal")?;

    if preferences.needs_setup() {
        let mut setup = SetupModel::new(settings, workspace_suggestions(&home));
        match run_setup(session.terminal(), &mut setup).await? {
            SetupOutcome::Cancelled => {
                drop(session);
                tracing::info!("setup cancelled, nothing written");
                return Ok(std::process::ExitCode::SUCCESS);
            }
            SetupOutcome::Done(choice) => {
                let initial = InitialSetup {
                    workspaces: choice.workspaces,
                    icons: match choice.icons {
                        IconSet::Unicode => IconPreference::Unicode,
                        IconSet::Nerd => IconPreference::Nerd,
                        IconSet::Ascii => IconPreference::Ascii,
                    },
                };
                match config.apply_setup(&initial).await {
                    Ok(saved) => {
                        tracing::info!(workspaces = saved.workspaces.len(), "setup saved");
                        preferences = saved;
                        settings.icons = choice.icons;
                    }
                    Err(error) => {
                        // Leave the alternate screen first so the message stays readable.
                        drop(session);
                        return Err(error).context("failed to save the setup");
                    }
                }
            }
        }
    }

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
    result.map(|()| std::process::ExitCode::SUCCESS)
}

fn config_store(home: &std::path::Path) -> TomlSettings {
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    TomlSettings::new(
        TomlSettings::default_path(home, xdg.as_deref()),
        home.to_path_buf(),
    )
}

/// Folders below the home directory that commonly hold projects and exist.
fn workspace_suggestions(home: &std::path::Path) -> Vec<String> {
    const CANDIDATES: [&str; 10] = [
        "workspace",
        "workspace/projects",
        "Projects",
        "projects",
        "Developer",
        "dev",
        "code",
        "src",
        "git",
        "repos",
    ];
    CANDIDATES
        .iter()
        .filter(|name| home.join(name).is_dir())
        .map(|name| format!("~/{name}"))
        .collect()
}

/// Invalid settings stop hud before the terminal is taken over, so the error
/// stays readable in the shell.
async fn load_settings(store: &TomlSettings) -> anyhow::Result<Settings> {
    let settings = store.load().await?;
    tracing::info!(
        path = %store.path().display(),
        workspaces = settings.workspaces.len(),
        needs_setup = settings.needs_setup(),
        "loaded settings"
    );
    Ok(settings)
}

/// Runs the first-run setup until it is finished or cancelled.
async fn run_setup(
    terminal: &mut terminal::Tui,
    model: &mut SetupModel,
) -> anyhow::Result<SetupOutcome> {
    let mut input = EventStream::new();
    loop {
        terminal.draw(|frame| view_setup(model, frame))?;
        let Some(event) = input.next().await else {
            return Ok(SetupOutcome::Cancelled);
        };
        if let Some(msg) = Msg::from_terminal(&event?)
            && let Some(outcome) = update_setup(model, &msg)
        {
            return Ok(outcome);
        }
    }
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
