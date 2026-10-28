//! What only the test images for the second OTA slot use, kept out of the real firmware's way:
//! their common start (`start`: the watchdog armed first, the latch held, the report begun, the
//! next boot handed back to the C++ firmware), the report they leave in RTC slow memory, a
//! watchdog that counts through their deep sleeps, their sleep marked on the report's last line
//! (`report_and_sleep`, `sleep_when_released`), and the touch panel's setup without an executor
//! (`open_touch`).

mod handback;
mod report;
mod sleep_report;
mod start;
mod touch;
mod watchdog;

pub use handback::{HandBack, hand_back};
pub use report::Report;
pub use sleep_report::{TestSleep, report_and_sleep, sleep_when_released};
pub use start::{Started, start};
pub use touch::{BlockingTouchBus, TouchStart, note_touch, open_touch};
pub use watchdog::{WATCHDOG_KEY, arm_watchdog, watch_sleep, watchdog_through_sleep};
