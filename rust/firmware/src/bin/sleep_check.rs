//! Sleep on the X4 Pro from the bare-metal firmware, step by step, for a device where a deep sleep
//! did not wake: hands the next boot back to the C++ firmware, then
//! 1. sleeps lightly for 3 s on the timer;
//! 2. sleeps lightly until Power is pressed, through the low-power path a deep sleep wakes on,
//!    or for 20 s;
//! 3. with Power up again, sleeps deeply until Power is pressed or for 15 s, the pins held as the
//!    SDK holds them for its own sleep (`sleep::deep_sleep`).
//!
//! A light sleep returns here, and the report says what ended it and how long it lasted. The deep
//! sleep's wake resets into the C++ firmware, which adds what woke the chip and what reset it. The
//! RTC watchdog counts on in every sleep (which esp-hal's `enable` would pause; see
//! `watchdog_through_sleep`): whatever hangs, it resets the chip into the C++ firmware, first
//! noting whether the CPU still ran (see `WATCHDOG_NOTE`). Without esp-rtos, so it runs in the
//! emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::gpio::{Event, Input, InputConfig, Level, Output, OutputConfig, Pull, WakeupConfig};
use esp_hal::peripherals::{Interrupt, LPWR};
use esp_hal::rtc_cntl::sleep::{LowPower, RtcSleepConfig};
use esp_hal::rtc_cntl::{RwdtStage, RwdtStageAction, WakeupSource, wakeup_cause};
use esp_hal::system::Cpu;
use esp_hal::time::{Duration, Instant};
use purestwhite_firmware::sleep::{self, Rails, WATCHDOG_KEY, watchdog_through_sleep};
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

/// The sleeps' lengths when nothing else ends them.
const LIGHT_TIMER: Duration = Duration::from_secs(3);
const LIGHT_POWER: Duration = Duration::from_secs(20);
const DEEP: Duration = Duration::from_secs(15);
/// How long Power may stay down before the deep sleep starts anyway.
const RELEASE_WAIT: Duration = Duration::from_secs(10);
/// The watchdog's stages from each step on, the first past every sleep here: an interrupt that
/// notes a CPU still running (a sleep never entered), then a core reset, which keeps the power
/// latch's hold and the report, and last a system reset, which releases the latch (on battery the
/// device then turns off) but ends a chip stuck asleep.
const WATCHDOG_NOTE: Duration = Duration::from_secs(60);
const WATCHDOG_CORE: Duration = Duration::from_secs(15);
const WATCHDOG_SYSTEM: Duration = Duration::from_secs(30);

/// What ended the last sleep, in a word.
fn cause() -> &'static str {
    let reason = wakeup_cause();
    if reason.contains(WakeupSource::Ext1) {
        "dugme"
    } else if reason.contains(WakeupSource::Timer) {
        "zamanlayici"
    } else if reason.is_empty() {
        "yok"
    } else {
        "diger"
    }
}

/// The watchdog's first stage: the CPU still runs, so a sleep was not entered or did not end as
/// a sleep should. Noted last in the report, where the C++ firmware looks for a sleep line.
#[esp_hal::handler]
fn watchdog_note() {
    Report::append_saved("uyku: girilmedi\n");
    let regs = LPWR::regs();
    // SAFETY: as in `watchdog_through_sleep`; this clears the stage's interrupt, which the next
    // stage does not need.
    regs.wdtwprotect()
        .write(|w| unsafe { w.bits(WATCHDOG_KEY) });
    regs.int_clr().write(|w| w.wdt().clear_bit_by_one());
    regs.wdtwprotect().write(|w| unsafe { w.bits(0) });
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal uyku denemesi");
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

    // From here on a hang, awake or asleep, resets into the C++ firmware (see `WATCHDOG_NOTE`).
    // esp-hal 1.2.2 halves every stage's time on the S3, though only stage 0 counts twice as long
    // in hardware (ESP-IDF halves stage 0 alone, in hal/esp32s3's rwdt_ll.h): the later stages get
    // twice the time meant.
    for (stage, action, timeout) in [
        (RwdtStage::Stage0, RwdtStageAction::Interrupt, WATCHDOG_NOTE),
        (
            RwdtStage::Stage1,
            RwdtStageAction::ResetCore,
            WATCHDOG_CORE * 2,
        ),
        (
            RwdtStage::Stage2,
            RwdtStageAction::ResetSystem,
            WATCHDOG_SYSTEM * 2,
        ),
    ] {
        rtc.rwdt.set_timeout(stage, timeout);
        rtc.rwdt.set_stage_action(stage, action);
    }
    rtc.set_interrupt_handler(watchdog_note);
    rtc.rwdt.listen();
    watchdog_through_sleep();
    rtc.rwdt.feed();

    let mut low_power = LowPower::new(peripherals.LPWR);

    // 1. A light sleep on the timer. Each sleep's line goes in first: if the sleep never ends, the
    // report ends with it, and the C++ firmware adds what reset the chip.
    let _ = writeln!(report, "uyku: hafif 3 s");
    report.save();
    let start = Instant::now();
    low_power.set_wakeup_deadline(start + LIGHT_TIMER);
    low_power.sleep_light(RtcSleepConfig::default());
    low_power.clear_wakeup_deadline();
    let _ = writeln!(
        report,
        "hafif 3 s: {}, {} ms",
        cause(),
        start.elapsed().as_millis()
    );
    report.save();
    rtc.rwdt.feed();

    // 2. A light sleep that Power ends, through the low-power path, with the timer behind it.
    // Power is active low, with the SDK's pull-up.
    let mut power = Input::new(
        peripherals.GPIO3,
        InputConfig::default().with_pull(Pull::Up),
    );
    power.listen(Event::LowLevel);
    // The low-power path wakes the chip without the pin's interrupt reaching the CPU, and nothing
    // here handles it (eink-qemu 0.9.2 also fails on GPIO interrupts).
    esp_hal::interrupt::disable(Cpu::ProCpu, Interrupt::GPIO);
    let low_power_path = WakeupConfig::default().with_low_power_path(true);
    if let Err(e) = power.apply_wakeup_config(&low_power_path) {
        let _ = writeln!(report, "guc kurulamadi ({e:?})");
    }
    let _ = writeln!(report, "uyku: hafif guc");
    report.save();
    esp_println::println!("uyku: guc bekleniyor");
    let start = Instant::now();
    low_power.set_wakeup_deadline(start + LIGHT_POWER);
    low_power.sleep_light(RtcSleepConfig::default());
    low_power.clear_wakeup_deadline();
    let _ = writeln!(
        report,
        "hafif guc: {}, {} ms",
        cause(),
        start.elapsed().as_millis()
    );
    report.save();
    rtc.rwdt.feed();

    // 3. Power up first: a wake pad at its wake level ends the sleep at once.
    if !sleep::wait_power_up(&power, RELEASE_WAIT, || {}) {
        // The sleep then ends at once, on Power.
        let _ = writeln!(report, "guc birakilmadi");
    }
    // The rails as the SDK leaves them for a deep sleep (see `sleep::deep_sleep`).
    let rails = Rails {
        touch_power: Output::new(peripherals.GPIO2, Level::High, OutputConfig::default()),
        sd_power: Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        panel_reset: Output::new(peripherals.GPIO14, Level::High, OutputConfig::default()),
    };
    let _ = writeln!(report, "uyku: derin, guc ya da 15 s");
    esp_println::println!("{}", report.as_str());
    report.save();
    rtc.rwdt.feed();
    // With a timer it does not return.
    let _ = sleep::deep_sleep(low_power, &mut power, rails, Some(DEEP), |_| {});
    esp_hal::system::software_reset()
}
