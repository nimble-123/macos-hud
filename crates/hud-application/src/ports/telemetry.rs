//! Telemetry from Claude Code, received while hud runs (ADR 0011).

use std::future::Future;
use std::sync::Arc;

use hud_domain::usage::{TelemetryPoint, TelemetryTotals};
use jiff::Timestamp;

use crate::ports::store::StoreError;

/// Keeps telemetry data points.
pub trait TelemetryStore: Send + Sync {
    /// Stores received points.
    fn save_points(
        &self,
        points: Vec<TelemetryPoint>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Sums of the points in `[from, to)`.
    fn totals(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<TelemetryTotals, StoreError>> + Send;
}

impl<T: TelemetryStore> TelemetryStore for Arc<T> {
    fn save_points(
        &self,
        points: Vec<TelemetryPoint>,
    ) -> impl Future<Output = Result<(), StoreError>> + Send {
        (**self).save_points(points)
    }

    fn totals(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<TelemetryTotals, StoreError>> + Send {
        (**self).totals(from, to)
    }
}

/// Whether the receiver could start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverState {
    /// Telemetry is switched off in the config.
    Off,
    /// Listening on `127.0.0.1` at this port.
    Listening(u16),
    /// The port could not be opened, e.g. because it is taken.
    Unavailable {
        /// Port.
        port: u16,
        /// Why.
        reason: String,
    },
}
