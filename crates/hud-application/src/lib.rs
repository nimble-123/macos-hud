//! Use cases and ports of hud.
//!
//! Ports are traits implemented by `hud-infrastructure`. This crate never
//! spawns processes, touches the file system or talks to SQLite.
#![warn(missing_docs)]

mod event;
pub mod ports;
pub mod settings;
pub mod usecases;

#[cfg(test)]
mod testing;

pub use event::{
    AppEvent, Command, LogCommand, ReportSource, SettingsCommand, UpdateCommand, WorkspaceCommand,
};
