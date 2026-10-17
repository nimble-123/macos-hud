//! [`UpdateSource`](hud_application::ports::updates::UpdateSource) adapters.

pub mod brew;
pub mod npm;
pub mod skills;

#[cfg(test)]
pub(crate) mod fake_runner;
