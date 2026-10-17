//! Use cases and ports of hud.
//!
//! Ports are traits implemented by `hud-infrastructure`. This crate never
//! spawns processes, touches the file system or talks to SQLite.
#![warn(missing_docs)]

mod event;

pub use event::{AppEvent, Command};
