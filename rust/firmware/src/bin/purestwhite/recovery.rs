//! The recovery mode (rust/firmware/DESIGN.md, the SD updater), entered before any task starts:
//! when the bootloader refused the C++ firmware's slot and fell back to this image, or when Right
//! is held as the image starts. It runs the firmware's `updater` (the C++ firmware's release on
//! the card, within phase 2's limits) with no screen, touch or font, then resets into the C++
//! firmware, or sleeps (bounded) when a reset would only bring the bootloader back here. The run
//! counts as healthy when it ends in that reset. The boot counter's "recovery due" is only noted
//! during phase 2: with the C++ firmware intact the updater could only write the same release
//! again.

use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::GPIO7;
use purestwhite_firmware::boot::{self, Started};
use purestwhite_firmware::updater::{self, Parts};

/// Why the recovery mode was entered, as the report names it.
#[derive(Clone, Copy, Debug)]
pub enum Why {
    /// The bootloader refused the C++ firmware's slot and started this image instead.
    FellBack,
    /// Right was held as the image started.
    Key,
}

impl Why {
    fn name(self) -> &'static str {
        match self {
            Why::FellBack => "fell back",
            Why::Key => "Right held",
        }
    }
}

/// Why to enter the recovery mode, if at all.
pub fn reason(fell_back: bool, right_held: bool) -> Option<Why> {
    if fell_back {
        Some(Why::FellBack)
    } else if right_held {
        Some(Why::Key)
    } else {
        None
    }
}

/// Whether Right (GPIO7, active low) is held as the image starts: low in each of five samples
/// 10 ms apart. The pin is given back for the keys afterwards.
pub fn right_held(pin: GPIO7<'_>) -> bool {
    let right = Input::new(pin, InputConfig::default().with_pull(Pull::Up));
    let delay = Delay::new();
    (0..5).all(|_| {
        let low = right.is_low();
        delay.delay_millis(10);
        low
    })
}

/// The recovery mode, noted (`recovery: <why>`): the updater (whether the bootloader fell back
/// decides whether it may repair the C++ firmware's slot), then a reset or the bounded sleep
/// (`updater::end`). The run is healthy when it ends in that reset. Unlike install-check it does
/// not note an earlier run's last line: with this image's `runs:` and `recovery:` lines it would
/// not leave room for the sleep line. A failing update whose lines are all at their longest can
/// still fill the report before the sleep line's step marks (the line itself starts within it).
pub fn run(why: Why, parts: Parts, started: &mut Started<'static>) -> ! {
    started
        .report
        .note(format_args!("recovery: {}", why.name()));
    let fell_back = started.found.fell_back();
    let (storage, rtc, report) = (&mut started.flash, &mut started.rtc, &mut started.report);
    let finished = updater::install(parts, storage, rtc, report, fell_back);
    if finished.resets {
        boot::finish_run();
    }
    updater::end(&mut started.report, &mut started.rtc, finished)
}
