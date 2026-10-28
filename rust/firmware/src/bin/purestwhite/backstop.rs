//! During phase 2 the image comes back to the C++ firmware without the owner: `MOST` after the
//! start the backstop asks the application for the sleep itself (as Power held does), unless the
//! sleep has begun; and if the power loop has not begun the sleep `GRACE` after that, it notes it
//! and resets into the C++ firmware. It goes away in phase 3, when this image is the firmware.

use crate::power::{FEED, SLEEPING, TIME_UP};
use crate::tasks::{LINKS, PANEL_LIMIT};
use core::sync::atomic::Ordering;
use embassy_time::{Duration, Instant, Timer};
use purestwhite_app::Event;
use purestwhite_firmware::boot::Report;

/// How long after the start the backstop asks for the sleep.
const MOST: Duration = Duration::from_secs(10 * 60);
/// How long after that the power loop may take to begin the sleep.
const GRACE: Duration = Duration::from_secs(60);
/// How often a request that did not fit in the events channel is tried again.
const RETRY: Duration = Duration::from_secs(1);

// The power loop sees the sleep signal at once; before that the panel may finish a frame under
// way, then show the sleep screen, each within `PANEL_LIMIT`. `FEED` is margin.
const _: () = assert!(GRACE.as_ticks() > 2 * PANEL_LIMIT.as_ticks() + FEED.as_ticks());

#[embassy_executor::task]
pub async fn backstop() {
    let ask_at = Instant::now() + MOST;
    let give_up_at = ask_at + GRACE;
    Timer::at(ask_at).await;
    // The events channel is full only for a moment; tried again until the grace ends.
    while !SLEEPING.load(Ordering::Acquire) && Instant::now() < give_up_at {
        if LINKS.events.try_send(Event::SleepRequested).is_ok() {
            TIME_UP.store(true, Ordering::Release);
            break;
        }
        Timer::after(RETRY).await;
    }
    Timer::at(give_up_at).await;
    if !SLEEPING.load(Ordering::Acquire) {
        Report::append_saved("no sleep after time up\n");
        esp_hal::system::software_reset();
    }
}
