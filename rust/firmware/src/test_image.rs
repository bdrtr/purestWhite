//! What only the test images for the second OTA slot use, kept out of the real firmware's way: a
//! watchdog that counts through their deep sleeps, their sleep marked on the report's last line
//! (`report_and_sleep`, `sleep_when_released`), and the touch panel's setup without an executor
//! (`open_touch`). Their common start is `crate::boot`'s.

mod sleep_report;
mod touch;
mod watchdog;

pub use sleep_report::{TestSleep, report_and_sleep, sleep_when_released};
pub use touch::{BlockingTouchBus, TouchStart, note_touch, open_touch};
pub use watchdog::{WATCHDOG_KEY, watch_sleep, watchdog_through_sleep};
