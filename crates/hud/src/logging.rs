//! File logging under `~/Library/Logs/hud/`; stdout belongs to the TUI.

use std::path::PathBuf;

use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

const LOG_FILTER_ENV: &str = "HUD_LOG";
const KEEP_LOG_FILES: usize = 7;

/// Installs the global subscriber. Keep the guard alive until exit so buffered
/// lines are flushed.
pub fn init() -> anyhow::Result<WorkerGuard> {
    let directory = log_dir()?;
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create log directory {}", directory.display()))?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("hud.log")
        .max_log_files(KEEP_LOG_FILES)
        .build(&directory)
        .context("cannot open log file")?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env(LOG_FILTER_ENV).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .init();
    Ok(guard)
}

fn log_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("Library/Logs/hud"))
}
