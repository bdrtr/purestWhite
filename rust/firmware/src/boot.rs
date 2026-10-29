//! The start every image in the second OTA slot shares while the C++ firmware is the known-good
//! one (phase 2): the RTC watchdog armed first, the power latch held, the report begun, and the
//! next boot handed back to the C++ firmware (`start`); the report the image leaves in RTC slow
//! memory for the C++ firmware to save; and the deep sleep bounded by a timer and the watchdog,
//! marked on the report (`report_and_sleep`), so that the image comes back to the C++ firmware
//! even when the sleep does not end; and the boot counter's record beside the report
//! (`begin_run`, `finish_run`).

mod bounded_sleep;
mod handback;
mod report;
mod runs;
mod start;
mod watchdog;

pub use bounded_sleep::{BoundedSleep, report_and_sleep, sleep_when_released};
pub(crate) use handback::select_other_slot;
pub use handback::{HandBack, hand_back};
pub use report::Report;
pub use runs::{begin_run, finish_run};
pub use start::{Started, start};
pub use watchdog::{WATCHDOG_KEY, arm_watchdog, watch_sleep, watchdog_through_sleep};
