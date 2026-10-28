//! The start every image in the second OTA slot shares while the C++ firmware is the known-good
//! one (phase 2): the RTC watchdog armed first, the power latch held, the report begun, and the
//! next boot handed back to the C++ firmware (`start`), and the report the image leaves in RTC
//! slow memory for the C++ firmware to save.

mod handback;
mod report;
mod start;
mod watchdog;

pub use handback::{HandBack, hand_back};
pub use report::Report;
pub use start::{Started, start};
pub use watchdog::arm_watchdog;
