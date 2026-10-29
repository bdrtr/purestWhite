//! What only the test images for the second OTA slot use, kept out of the real firmware's way: the
//! touch panel's setup without an executor (`open_touch`) and its report line, a report line saved
//! and printed at once (`note`), and selecting this image's own slot for the next boot
//! (`select_this_image`, which only boot-check uses). Their common start and their sleep are
//! `crate::boot`'s.

mod note;
mod select;
mod touch;

pub use note::note;
pub use select::select_this_image;
pub use touch::{BlockingTouchBus, TouchStart, note_touch, open_touch};
