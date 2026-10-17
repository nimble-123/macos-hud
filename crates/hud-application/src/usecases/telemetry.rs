//! Storing telemetry as it arrives.

use hud_domain::usage::TelemetryPoint;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ports::telemetry::TelemetryStore;

/// Stores batches of telemetry points sent by the receiver.
pub struct TelemetryAgent<S> {
    store: S,
}

impl<S: TelemetryStore> TelemetryAgent<S> {
    /// Agent over the store.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Stores every batch until cancelled or the receiver is gone.
    pub async fn run(
        self,
        mut points: mpsc::Receiver<Vec<TelemetryPoint>>,
        cancel: CancellationToken,
    ) {
        loop {
            let batch = tokio::select! {
                () = cancel.cancelled() => return,
                batch = points.recv() => batch,
            };
            let Some(batch) = batch else { return };
            if let Err(error) = self.store.save_points(batch).await {
                tracing::warn!(%error, "could not store telemetry");
            }
        }
    }
}
