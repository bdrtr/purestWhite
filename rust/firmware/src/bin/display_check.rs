//! The X4 Pro's panel from the bare-metal firmware: hands the next boot back to the C++
//! firmware, probes which controller drives the panel (the bytes the C++ firmware's probe sends
//! at every boot), and on a UC8279 or UC8179 draws a test pattern with purestwhite-epd's driver
//! (a full refresh, held 2 s, then a fast one with the block moved). On a UC8179 four gray bands
//! follow, in CrossPoint's grayscale passes: two reader pages (the second with the bands reversed,
//! after the non-flashing transition), an image's and the sleep screen's. From the fast pattern
//! on each screen is held 3 s; then the controller sleeps and the chip resets. After a step that
//! fails nothing more goes to the controller; 50 s after boot no call starts, and one under way
//! stops at its next wait. On an
//! SSD1677, which has no driver here yet, nothing goes to it after the probe.
//! Without esp-rtos, so it runs in the emulator too: the driver's async calls go over blocking
//! SPI and delays, ready at once.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::time::{Duration, Instant};
use purestwhite_epd::probe::{Controller, Probe, Verdict};
use purestwhite_epd::uc8179::GrayMode;
use purestwhite_epd::uc8279x4::Refresh;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::board::PanelParts;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::panel::{self, Lines, OpenError, Panel, finish_by};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIC\n");
    esp_hal::system::software_reset()
}

/// White with a 16-pixel black border, a black block whose left edge is `block` bytes in, and
/// bars across the middle; a bit per pixel, 1 white.
fn pattern(frame: &mut [u8], block: usize) {
    for (y, row) in frame.chunks_exact_mut(100).enumerate() {
        row.fill(0xFF);
        if !(16..464).contains(&y) {
            row.fill(0x00);
            continue;
        }
        for edge in [0, 1, 98, 99] {
            if let Some(b) = row.get_mut(edge) {
                *b = 0x00;
            }
        }
        if (100..380).contains(&y) {
            for b in row.iter_mut().skip(block).take(20) {
                *b = 0x00;
            }
        }
        if (200..280).contains(&y) {
            for (i, b) in row.iter_mut().enumerate().take(92).skip(50) {
                if (i / 5) % 2 == 0 {
                    *b = 0x00;
                }
            }
        }
    }
}

/// The controller found, with the probe's verdict and raw bytes.
fn note(report: &mut Report, found: &Probe) {
    let verdict = match found.verdict {
        Verdict::PrimaryAssumed => 0,
        Verdict::Uc81xxConfirmed => 1,
        Verdict::Inconclusive => 2,
    };
    let [v0, v1, v2, v3, v4] = found.ver;
    let _ = writeln!(
        report,
        "panel: {} k{verdict} ver {v0:02x}{v1:02x}{v2:02x}{v3:02x}{v4:02x} flg {:02x}",
        found.controller().name(),
        found.flg
    );
}

/// Bytes in a frame: 480 rows of 100.
const FRAME_BYTES: usize = 100 * 480;

/// The grayscale steps on a UC8179: the pass, the bands' levels from the start of each row (0
/// black to 3 white), and the step's name on the serial port.
const GRAY_STEPS: [(GrayMode, [u8; 4], &str); 4] = [
    (GrayMode::Overlay, [0, 1, 2, 3], "gray text 1"),
    (GrayMode::Overlay, [3, 2, 1, 0], "gray text 2"),
    (GrayMode::Absolute, [0, 1, 2, 3], "gray image"),
    (GrayMode::Direct, [0, 1, 2, 3], "gray direct"),
];

/// Four bands across each row, 200 pixels each, of `levels`, as CrossPoint encodes them: the base
/// (`None`) white only for white, and the LSB and MSB planes (`Some(msb)`) as `mode` takes them.
fn bands(frame: &mut [u8], levels: [u8; 4], mode: GrayMode, plane: Option<bool>) {
    for row in frame.chunks_exact_mut(100) {
        for (part, &level) in row.chunks_exact_mut(25).zip(&levels) {
            let set = match (plane, mode) {
                (None, _) => level == 3,
                (Some(false), GrayMode::Overlay) => level == 1,
                (Some(true), GrayMode::Overlay) => level == 1 || level == 2,
                (Some(false), _) => level == 1 || level == 3,
                (Some(true), _) => level >= 2,
            };
            part.fill(if set { 0xFF } else { 0x00 });
        }
    }
}

/// A frame-long buffer that lives as long as the program, or `None` when the heap has no room.
fn lasting_frame() -> Option<&'static mut [u8]> {
    let mut v = Vec::new();
    v.try_reserve_exact(FRAME_BYTES).ok()?;
    v.resize(FRAME_BYTES, 0);
    Some(v.leak())
}

/// "ok" or "failed".
fn outcome(ok: bool) -> &'static str {
    if ok { "ok" } else { "failed" }
}

/// The panel's controller, probed with the panel lines as GPIOs for a moment, and its driver over
/// the lines as SPI; the controller noted in the report. Resets when there is no driver for it
/// (nothing more goes to it) or a step fails.
fn open_panel(report: &mut Report, parts: PanelParts) -> (Panel, Lines<'static>) {
    let base = |c| {
        matches!(c, Controller::Uc8179(_))
            .then(lasting_frame)
            .flatten()
    };
    let error = match panel::open(parts, base) {
        Ok(opened) => {
            note(report, &opened.probe);
            report.save();
            return (opened.panel, opened.lines);
        }
        Err(error) => error,
    };
    if let Some(found) = error.probe() {
        note(report, found);
        report.save();
    }
    let line = match error {
        OpenError::Probe => "panel: probe failed",
        OpenError::NoDriver(_) => "panel: skipped",
        OpenError::Spi(_) => "panel: no spi",
    };
    let _ = writeln!(report, "{line}");
    if matches!(error, OpenError::NoDriver(_)) {
        let _ = writeln!(report, "done");
        esp_println::println!("{}", report.as_str());
    }
    report.save();
    esp_hal::system::software_reset()
}

/// The test pattern with a full refresh, held 2 s, then with the block moved and a fast one,
/// held 3 s; whether both worked.
fn show_patterns(
    report: &mut Report,
    panel: &mut Panel,
    bus: &mut Lines<'_>,
    frame: &mut [u8],
) -> bool {
    let delay = Delay::new();
    pattern(frame, 10);
    let full = panel.begin(bus) && panel.display(bus, frame, Refresh::Full, false);
    let _ = writeln!(report, "panel full: {}", outcome(full));
    report.save();
    esp_println::println!("panel full: {full}");
    if !full {
        return false;
    }
    delay.delay_millis(2000);
    pattern(frame, 60);
    let fast = panel.display(bus, frame, Refresh::Fast, true);
    let _ = writeln!(report, "panel fast: {}", outcome(fast));
    report.save();
    esp_println::println!("panel fast: {fast}");
    if fast {
        delay.delay_millis(3000);
    }
    fast
}

/// The gray bands of `GRAY_STEPS`, each held 3 s, on one report line with a word per step;
/// whether every step worked (the steps stop at the first that fails).
fn show_grays(
    report: &mut Report,
    panel: &mut Panel,
    bus: &mut Lines<'_>,
    frame: &mut [u8],
) -> bool {
    let delay = Delay::new();
    let label = if panel.has_gray_base() {
        "gray"
    } else {
        "gray no base"
    };
    let _ = write!(report, "{label}:");
    let mut held = true;
    for (mode, levels, name) in GRAY_STEPS {
        held = panel.gray(bus, frame, mode, |f, plane| {
            bands(f, levels, mode, plane);
        });
        let _ = write!(report, " {}", outcome(held));
        report.save();
        esp_println::println!("{name}: {held}");
        if !held {
            break;
        }
        delay.delay_millis(3000);
    }
    let _ = writeln!(report);
    held
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let started = Instant::now();
    let Started {
        rtc: _rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal display check", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    // Room for the UC8179's grayscale base (the SDK's PSRAM copy) beside the frame.
    esp_alloc::heap_allocator!(size: 50 * 1024);

    let pins = board.panel;
    let (mut panel, mut bus) = open_panel(&mut report, pins);
    // Well before the 60 s watchdog, however slowly each call finishes.
    finish_by(&mut bus, started + Duration::from_secs(50));
    let mut frame = vec![0xFFu8; FRAME_BYTES];
    let mut held = show_patterns(&mut report, &mut panel, &mut bus, &mut frame);
    if held && matches!(panel, Panel::Uc8179(_)) {
        held = show_grays(&mut report, &mut panel, &mut bus, &mut frame);
    }
    if held {
        let slept = panel.deep_sleep(&mut bus);
        let _ = writeln!(report, "panel sleep: {}", outcome(slept));
    }
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
