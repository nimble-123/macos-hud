//! Domain model of hud.
//!
//! Rules are pure functions without I/O. This crate must not depend on Tokio,
//! Serde formats, Ratatui or any other infrastructure.
#![warn(missing_docs)]

pub mod action;
pub mod git;
pub mod update;
pub mod usage;
mod workspace;

pub use workspace::{DomainError, WorkspacePath};
