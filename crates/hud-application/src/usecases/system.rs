//! Sampling the machine's load for the overview.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::event::AppEvent;
use crate::ports::system::SystemProbe;

/// Sends a [`SystemSample`](crate::ports::system::SystemSample) every interval.
pub struct SystemAgent<P> {
    probe: P,
    interval: Duration,
}

impl<P: SystemProbe> SystemAgent<P> {
    /// Agent sampling `probe` every `interval`.
    pub fn new(probe: P, interval: Duration) -> Self {
        Self { probe, interval }
    }

    /// Samples until cancelled or the UI is gone.
    pub async fn run(self, events: mpsc::Sender<AppEvent>, cancel: CancellationToken) {
        let mut timer = tokio::time::interval(self.interval);
        // A slow sample must not cause a burst of catch-up samples.
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = cancel.cancelled() => return,
                _ = timer.tick() => {}
            }
            let sample = self.probe.sample().await;
            if events.send(AppEvent::System(sample)).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::system::SystemSample;

    struct FixedProbe;

    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the fake answers immediately; the trait still needs async signatures"
    )]
    impl SystemProbe for FixedProbe {
        async fn sample(&self) -> SystemSample {
            SystemSample {
                cpu_percent: 23,
                memory_used: 6,
                memory_total: 10,
                processes: 512,
            }
        }
    }

    #[tokio::test]
    async fn the_agent_sends_samples() {
        let (events_tx, mut events) = mpsc::channel(2);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(
            SystemAgent::new(FixedProbe, Duration::from_millis(10)).run(events_tx, cancel.clone()),
        );
        let Some(AppEvent::System(sample)) = events.recv().await else {
            panic!("expected a sample")
        };
        assert_eq!(sample.memory_percent(), 60);
        cancel.cancel();
        task.await.expect("BUG");
    }

    #[test]
    fn memory_percent_handles_unknown_totals() {
        let sample = SystemSample {
            cpu_percent: 0,
            memory_used: 5,
            memory_total: 0,
            processes: 0,
        };
        assert_eq!(sample.memory_percent(), 0);
    }
}
