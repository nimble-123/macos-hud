//! [`Clock`] adapter.

use hud_application::ports::store::Clock;
use jiff::Timestamp;

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}
