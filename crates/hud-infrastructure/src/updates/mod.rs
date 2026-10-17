//! [`UpdateSource`](hud_application::ports::updates::UpdateSource) adapters.

pub mod brew;
pub mod npm;

#[cfg(test)]
pub(crate) mod fake_runner;
