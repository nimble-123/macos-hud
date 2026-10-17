//! [`SystemProbe`] over `sysinfo`.

use std::sync::{Arc, Mutex};

use hud_application::ports::system::{SystemProbe, SystemSample};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// Reads CPU, memory and process count. CPU load needs the previous reading,
/// so the same `System` is reused between samples.
#[derive(Debug, Clone)]
pub struct SysinfoProbe {
    system: Arc<Mutex<System>>,
}

impl Default for SysinfoProbe {
    fn default() -> Self {
        Self {
            system: Arc::new(Mutex::new(System::new())),
        }
    }
}

impl SystemProbe for SysinfoProbe {
    async fn sample(&self) -> SystemSample {
        let system = Arc::clone(&self.system);
        // sysinfo reads kernel tables synchronously.
        tokio::task::spawn_blocking(move || {
            let mut system = system
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            system.refresh_cpu_usage();
            system.refresh_memory();
            system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing(),
            );
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 0..=100 before the cast"
            )]
            let cpu_percent = system.global_cpu_usage().clamp(0.0, 100.0).round() as u8;
            SystemSample {
                cpu_percent,
                memory_used: system.used_memory(),
                memory_total: system.total_memory(),
                processes: u32::try_from(system.processes().len()).unwrap_or(u32::MAX),
            }
        })
        .await
        .unwrap_or(SystemSample {
            cpu_percent: 0,
            memory_used: 0,
            memory_total: 0,
            processes: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_real_sample_has_memory_and_processes() {
        let sample = SysinfoProbe::default().sample().await;
        assert!(sample.memory_total > 0);
        assert!(sample.memory_used <= sample.memory_total);
        assert!(sample.processes > 0);
    }
}
