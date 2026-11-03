//! What only the test images for the second OTA slot use, kept out of the real firmware's way: the
//! touch panel's setup without an executor (`open_touch`) and its report line, and selecting this
//! image's own slot for the next boot (`select_this_image`, which only boot-check uses), and the
//! clock and random bytes for TLS (`tls`). Their
//! common start and their sleep are `crate::boot`'s.

mod select;
#[cfg(feature = "wifi")]
pub mod tls;
mod touch;

pub use select::select_this_image;
pub use touch::{BlockingTouchBus, TouchStart, note_touch, open_touch};
