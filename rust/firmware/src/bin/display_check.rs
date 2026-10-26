//! The X4 Pro's panel from the bare-metal firmware: hands the next boot back to the C++
//! firmware, probes which controller drives the panel (the bytes the C++ firmware's probe sends
//! at every boot), and on a UC8279 or UC8179 draws a test pattern with purestwhite-epd's driver
//! (a full refresh, then a fast one with the block moved), holds it 3 s, puts the controller to
//! sleep and resets; after a step that fails, it sends nothing more. On an SSD1677, which has no
//! driver here yet, it sends nothing after the probe.
//! Without esp-rtos, so it runs in the emulator too: the driver's async calls go over blocking
//! SPI and delays, ready at once.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::vec;
use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use purestwhite_epd::probe::{Probe, Verdict};
use purestwhite_epd::uc8279x4::Refresh;
use purestwhite_firmware::panel::{Panel, PanelPins};
use purestwhite_firmware::{Report, arm_watchdog, hand_back, hold_power_latch};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIK\n");
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

/// "tamam" or "hata".
fn outcome(ok: bool) -> &'static str {
    if ok { "tamam" } else { "hata" }
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal ekran denemesi");
    report.save();
    match hand_back(peripherals.FLASH, peripherals.CPU_CTRL) {
        Ok(_) => {
            let _ = writeln!(report, "sonraki acilis: C++ yazilimi");
        }
        Err(e) => {
            // The way back rests on the bootloader alone now: touch nothing more.
            let _ = writeln!(report, "sonraki acilis: SECILEMEDI ({e})");
            report.save();
            esp_hal::system::software_reset()
        }
    }
    report.save();
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // Which controller: the panel lines as GPIOs for a moment, then as SPI below.
    let mut pins = PanelPins {
        spi: peripherals.SPI2,
        sclk: peripherals.GPIO12,
        mosi: peripherals.GPIO11,
        cs: peripherals.GPIO13,
        dc: peripherals.GPIO18,
        rst: peripherals.GPIO14,
        busy: peripherals.GPIO6,
    };
    let Some(found) = pins.probe() else {
        let _ = writeln!(report, "panel: yoklanamadi");
        report.save();
        esp_hal::system::software_reset()
    };
    note(&mut report, &found);
    report.save();
    let Some(mut panel) = Panel::for_controller(found.controller()) else {
        // No driver for this controller yet: nothing more goes to it.
        let _ = writeln!(report, "ekran: atlandi");
        let _ = writeln!(report, "bitti");
        esp_println::println!("{}", report.as_str());
        report.save();
        esp_hal::system::software_reset()
    };
    let Some(mut bus) = pins.open() else {
        let _ = writeln!(report, "ekran: spi kurulamadi");
        report.save();
        esp_hal::system::software_reset()
    };

    let mut frame = vec![0xFFu8; 100 * 480];
    let delay = Delay::new();
    pattern(&mut frame, 10);
    let full = panel.begin(&mut bus) && panel.display(&mut bus, &frame, Refresh::Full, false);
    let _ = writeln!(report, "ekran tam: {}", outcome(full));
    report.save();
    esp_println::println!("ekran tam: {full}");
    if full {
        delay.delay_millis(2000);
        pattern(&mut frame, 60);
        let fast = panel.display(&mut bus, &frame, Refresh::Fast, true);
        let _ = writeln!(report, "ekran hizli: {}", outcome(fast));
        report.save();
        esp_println::println!("ekran hizli: {fast}");
        if fast {
            delay.delay_millis(3000);
            let slept = panel.deep_sleep(&mut bus);
            let _ = writeln!(report, "ekran uyku: {}", outcome(slept));
        }
    }
    let _ = writeln!(report, "bitti");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
