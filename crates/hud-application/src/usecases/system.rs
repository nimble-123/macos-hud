//! Sampling the machine's load and free disk space for the overview.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::event::AppEvent;
use crate::ports::system::SystemProbe;

/// Sends a [`SystemSample`](crate::ports::system::SystemSample) every
/// interval and the [`Volume`](crate::ports::system::Volume)s less often:
/// free space changes slowly.
pub struct SystemAgent<P> {
    probe: P,
    interval: Duration,
    volume_interval: Duration,
}

impl<P: SystemProbe> SystemAgent<P> {
    /// Agent sampling `probe` every `interval` and its volumes every `volume_interval`.
    pub fn new(probe: P, interval: Duration, volume_interval: Duration) -> Self {
        Self {
            probe,
            interval,
            volume_interval,
        }
    }

    /// Samples until cancelled or the UI is gone.
    pub async fn run(self, events: mpsc::Sender<AppEvent>, cancel: CancellationToken) {
        let mut timer = tokio::time::interval(self.interval);
        let mut volume_timer = tokio::time::interval(self.volume_interval);
        // A slow sample must not cause a burst of catch-up samples.
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        volume_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let event = tokio::select! {
                () = cancel.cancelled() => return,
                _ = timer.tick() => AppEvent::System(self.probe.sample().await),
                _ = volume_timer.tick() => AppEvent::Volumes(self.probe.volumes().await),
            };
            if events.send(event).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::ports::system::{SystemSample, Volume};

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

        async fn volumes(&self) -> Vec<Volume> {
            vec![Volume {
                name: "Macintosh HD".into(),
                mount: PathBuf::from("/"),
                available: 1,
                total: 10,
            }]
        }
    }

    #[tokio::test]
    async fn the_agent_sends_samples_and_volumes() {
        let (events_tx, mut events) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let agent = SystemAgent::new(
            FixedProbe,
            Duration::from_millis(10),
            Duration::from_millis(10),
        );
        let task = tokio::spawn(agent.run(events_tx, cancel.clone()));
        let (mut sample, mut volumes) = (None, None);
        while sample.is_none() || volumes.is_none() {
            match events.recv().await.expect("BUG: agent runs") {
                AppEvent::System(reading) => sample = Some(reading),
                AppEvent::Volumes(list) => volumes = Some(list),
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(sample.map(|s| s.memory_percent()), Some(60));
        assert_eq!(volumes.map(|v| v.len()), Some(1));
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
