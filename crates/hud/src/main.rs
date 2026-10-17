//! hud binary: composition root, terminal lifecycle and event loop.

mod cli;
mod environment;
mod logging;
mod reset;
mod terminal;

use std::time::Duration;

use anyhow::Context;
use crossterm::event::EventStream;
use futures::StreamExt;
use std::path::PathBuf;

use hud_application::settings::{IconPreference, InitialSetup, Settings, SettingsStore};
use std::sync::Arc;

use hud_application::ports::updates::UpdateSource;
use hud_application::usecases::agent::WorkspaceAgent;
use hud_application::usecases::settings::SettingsAgent;
use hud_application::usecases::updates::UpdatesAgent;
use hud_application::{AppEvent, Command, SettingsCommand, UpdateCommand, WorkspaceCommand};
use hud_infrastructure::clock::SystemClock;
use hud_infrastructure::config::TomlSettings;
use hud_infrastructure::discovery::FsDiscovery;
use hud_infrastructure::git::GitCli;
use hud_infrastructure::process::TokioProcessRunner;
use hud_infrastructure::sqlite::SqliteStore;
use hud_infrastructure::updates::{brew::Brew, npm::NpmGlobal, skills::Skills};
use hud_tui::{
    IconSet, Model, Msg, SetupModel, SetupOutcome, update, update_setup, view, view_setup,
};
use tokio::sync::{mpsc, watch};
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
        cli::Action::Reset(choice) => return run_reset(choice),
    }

    let _log_guard = logging::init().context("failed to initialise logging")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting hud");

    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let config = config_store(&home);
    let preferences = load_settings(&config).await?;

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

    let system = environment::system();
    let settings = hud_tui::UiSettings::resolve(&preferences, system, environment::color_depth());
    let mut session = terminal::Session::enter().context("failed to prepare the terminal")?;
    let (preferences, settings) = if preferences.needs_setup() {
        match ensure_setup(&mut session, &config, &home, settings).await {
            Ok(Some(done)) => done,
            Ok(None) => {
                drop(session);
                tracing::info!("setup cancelled, nothing written");
                return Ok(std::process::ExitCode::SUCCESS);
            }
            Err(error) => {
                // Leave the alternate screen first so the message stays readable.
                drop(session);
                return Err(error);
            }
        }
    } else {
        (preferences, settings)
    };

    let mut background = start_background(preferences, config, runner)?;
    let result = run(
        session.terminal(),
        Model::new(settings).with_system(system),
        &mut background.events,
        &background.commands,
    )
    .await;
    drop(session);

    background.cancel.cancel();
    drop(background.commands);
    if tokio::time::timeout(
        SHUTDOWN_GRACE,
        futures::future::join_all(background.handles),
    )
    .await
    .is_err()
    {
        tracing::warn!("background work still running at exit");
    }
    tracing::info!("stopping hud");
    result.map(|()| std::process::ExitCode::SUCCESS)
}

/// Deletes the chosen files after confirmation; never starts the UI.
fn run_reset(choice: cli::Reset) -> anyhow::Result<std::process::ExitCode> {
    use std::io::IsTerminal;
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let paths = reset::targets(choice, config_store(&home).path(), &database_path()?);
    if !choice.yes {
        // Without a terminal nobody can answer; deleting anyway would be a surprise.
        if !std::io::stdin().is_terminal() {
            eprintln!("refusing to delete without confirmation; pass --yes");
            return Ok(std::process::ExitCode::from(2));
        }
        if !reset::confirm(&paths, std::io::stdin().lock(), std::io::stdout())? {
            println!("Nothing deleted.");
            return Ok(std::process::ExitCode::SUCCESS);
        }
    }
    for (path, removal) in reset::remove(&paths)? {
        match removal {
            reset::Removal::Deleted => println!("deleted  {}", path.display()),
            reset::Removal::Missing => println!("missing  {}", path.display()),
        }
    }
    Ok(std::process::ExitCode::SUCCESS)
}

/// Runs the first-run setup; `None` when the user cancelled it.
async fn ensure_setup(
    session: &mut terminal::Session,
    config: &TomlSettings,
    home: &std::path::Path,
    mut settings: hud_tui::UiSettings,
) -> anyhow::Result<Option<(Settings, hud_tui::UiSettings)>> {
    let mut setup = SetupModel::new(settings, workspace_suggestions(home));
    let SetupOutcome::Done(choice) = run_setup(session.terminal(), &mut setup).await? else {
        return Ok(None);
    };
    let initial = InitialSetup {
        workspaces: choice.workspaces,
        icons: match choice.icons {
            IconSet::Unicode => IconPreference::Unicode,
            IconSet::Nerd => IconPreference::Nerd,
            IconSet::Ascii => IconPreference::Ascii,
        },
    };
    let saved = config
        .apply_setup(&initial)
        .await
        .context("failed to save the setup")?;
    tracing::info!(workspaces = saved.workspaces.len(), "setup saved");
    settings.icons = choice.icons;
    Ok(Some((saved, settings)))
}

/// Background agents and the channels to talk to them.
struct Background {
    commands: Router,
    events: mpsc::Receiver<AppEvent>,
    cancel: CancellationToken,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

/// Composition root: the only place where adapters meet use cases.
fn start_background(
    preferences: Settings,
    config: TomlSettings,
    runner: TokioProcessRunner,
) -> anyhow::Result<Background> {
    let store = SqliteStore::open(&database_path()?, jiff::Timestamp::now())
        .context("failed to open the database")?;
    let git_program = preferences
        .tools
        .git
        .as_ref()
        .map_or_else(|| "git".to_owned(), |path| path.display().to_string());
    // Tool paths and enabled sources are read once; changing them needs a restart.
    let sources = update_sources(&preferences, &runner);
    let (current, watched) = watch::channel(preferences);
    let workspaces = WorkspaceAgent::new(
        GitCli::new(runner, git_program),
        FsDiscovery,
        store.clone(),
        store.clone(),
        SystemClock,
        watched.clone(),
    );
    let (events_tx, events) = mpsc::channel::<AppEvent>(APP_EVENT_CAPACITY);
    let (workspace_commands, workspace_rx) = mpsc::channel::<WorkspaceCommand>(COMMAND_CAPACITY);
    let (update_commands, update_rx) = mpsc::channel::<UpdateCommand>(COMMAND_CAPACITY);
    let (settings_commands, settings_rx) = mpsc::channel::<SettingsCommand>(COMMAND_CAPACITY);
    let updates = UpdatesAgent::new(
        sources,
        store.clone(),
        store,
        SystemClock,
        watched,
        events_tx.clone(),
    );
    let settings = SettingsAgent::new(config, current);
    let cancel = CancellationToken::new();
    let handles = vec![
        tokio::spawn(settings.run(settings_rx, events_tx.clone(), cancel.clone())),
        tokio::spawn(workspaces.run(workspace_rx, events_tx, cancel.clone())),
        tokio::spawn(updates.run(update_rx, cancel.clone())),
    ];
    Ok(Background {
        commands: Router {
            workspaces: workspace_commands,
            updates: update_commands,
            settings: settings_commands,
        },
        events,
        cancel,
        handles,
    })
}

/// Hands each command to the agent that handles it.
struct Router {
    workspaces: mpsc::Sender<WorkspaceCommand>,
    updates: mpsc::Sender<UpdateCommand>,
    settings: mpsc::Sender<SettingsCommand>,
}

impl Router {
    async fn send(&self, command: Command) -> anyhow::Result<()> {
        match command {
            Command::Workspaces(command) => self.workspaces.send(command).await?,
            Command::Updates(command) => self.updates.send(command).await?,
            Command::Settings(command) => self.settings.send(command).await?,
        }
        Ok(())
    }
}

/// Update sources in display order; Skills only when enabled in the config.
fn update_sources(
    preferences: &Settings,
    runner: &TokioProcessRunner,
) -> Vec<Arc<dyn UpdateSource>> {
    let program = |path: Option<&std::path::Path>, default: &str| {
        path.map_or_else(|| default.to_owned(), |path| path.display().to_string())
    };
    let npm = program(preferences.tools.npm.as_deref(), "npm");
    // npx lives next to npm; an explicit npm path implies its npx.
    let npx = preferences
        .tools
        .npm
        .as_deref()
        .and_then(std::path::Path::parent)
        .map_or_else(
            || "npx".to_owned(),
            |dir| dir.join("npx").display().to_string(),
        );
    let mut sources: Vec<Arc<dyn UpdateSource>> = vec![
        Arc::new(Brew::new(
            runner.clone(),
            program(preferences.tools.brew.as_deref(), "brew"),
        )),
        Arc::new(NpmGlobal::new(runner.clone(), npm)),
    ];
    if preferences.skills.enabled {
        sources.push(Arc::new(Skills::new(
            runner.clone(),
            npx,
            preferences.skills.cli_version.as_deref(),
        )));
    }
    sources
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
    commands: &Router,
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
