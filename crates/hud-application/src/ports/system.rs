//! Load of the machine for the overview.

use std::future::Future;

/// One reading of CPU, memory and processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemSample {
    /// CPU load over all cores, 0 to 100.
    pub cpu_percent: u8,
    /// Memory in use, bytes.
    pub memory_used: u64,
    /// Installed memory, bytes.
    pub memory_total: u64,
    /// Running processes.
    pub processes: u32,
}

impl SystemSample {
    /// Memory in use, 0 to 100.
    #[must_use]
    pub fn memory_percent(&self) -> u8 {
        if self.memory_total == 0 {
            return 0;
        }
        let percent = u128::from(self.memory_used) * 100 / u128::from(self.memory_total);
        u8::try_from(percent.min(100)).unwrap_or(100)
    }
}

/// Reads the current load.
pub trait SystemProbe: Send + Sync {
    /// A fresh reading.
    fn sample(&self) -> impl Future<Output = SystemSample> + Send;
}
