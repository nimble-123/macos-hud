//! Messages between the presentation layer and the use cases.

/// Result of background work, sent from use cases to the presentation layer.
///
/// No use case exists yet, so the enum has no variants. Matches on it are
/// exhaustive by construction and will fail to compile once variants appear.
#[derive(Debug)]
pub enum AppEvent {}

/// Request from the presentation layer to run a use case.
///
/// No use case exists yet, so the enum has no variants.
#[derive(Debug)]
pub enum Command {}
