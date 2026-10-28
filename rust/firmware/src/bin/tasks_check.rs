//! Embassy tasks on the X4 Pro from the bare-metal firmware: hands the next boot back to the C++
//! firmware, starts esp-rtos, then for up to 30 s runs three tasks side by side, each woken by an
//! interrupt or a timer rather than polling:
//! - buttons: Left (GPIO0), Right (GPIO7) and Power (GPIO3), debounced by `purestwhite-buttons`,
//!   woken when a pin leaves its level, a debounce ends or Power's hold reaches a second, which
//!   ends the 30 s early;
//! - SD: mounts the card over the interrupt-driven bus (`purestwhite_firmware::sd::AsyncBus`) and
//!   reads this image's file whole again and again, hashing each pass;
//! - heartbeat: wakes every 10 ms and notes how late it woke, which shows whether the others
//!   starve the executor.
//!
//! Then it reports and, once Power is up (or after 10 s), resets into the C++ firmware. On the device only:
//! eink-qemu 0.9.2 crashes on the GPIO and SDMMC interrupts.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select, select3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Delay, Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::sdmmc::{Config as SdConfig, SdHostController, SlotConfig};
use esp_hal::timer::systimer::SystemTimer;
use purestwhite_buttons::Buttons;
use purestwhite_fat::{Error, Volume};
use purestwhite_firmware::hold_power_latch;
use purestwhite_firmware::sd::{self, AsyncBus, AsyncCard, CardBlocks};
use purestwhite_firmware::test_image::{Report, arm_watchdog, hand_back};

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

/// How long the tasks run, and how long Power must be held to end that early.
const WINDOW: Duration = Duration::from_secs(30);
const LONG_MS: u32 = 1000;
/// How long the SD task may take to finish its pass after the window.
const SD_WAIT: Duration = Duration::from_secs(3);
/// How long Power may stay down after the window before the reset.
const RELEASE_WAIT: Duration = Duration::from_secs(10);
/// The heartbeat's period.
const BEAT: Duration = Duration::from_millis(10);
/// The buttons' bits in the sampled state.
const UP: u8 = 1 << 0;
const DOWN: u8 = 1 << 1;
const POWER_BIT: u8 = 2;
const POWER: u8 = 1 << POWER_BIT;
/// This image's file, read whole: its name on the owner's card.
const IMAGE: &str = "purestwhite-baremetal-gorev.bin";
/// FNV-1a's 32-bit offset basis and prime.
const FNV_BASIS: u32 = 0x811C_9DC5;
const FNV_PRIME: u32 = 0x0100_0193;

/// Set when the window ends: the SD task stops after its pass.
static STOP: AtomicBool = AtomicBool::new(false);
/// Set by the buttons task when Power is held long: the window ends.
static LONG: AtomicBool = AtomicBool::new(false);
/// The buttons task's counts: presses of Left, Right and Power; wakes by a pin and by a timer.
static PRESSES: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static PIN_WAKES: AtomicU32 = AtomicU32::new(0);
static TIMER_WAKES: AtomicU32 = AtomicU32::new(0);
/// Whether Power is down, debounced.
static POWER_DOWN: AtomicBool = AtomicBool::new(false);
/// The heartbeat's wakes, and the latest of them past its time, in microseconds.
static BEATS: AtomicU32 = AtomicU32::new(0);
static LATEST_US: AtomicU32 = AtomicU32::new(0);
/// The SD task's report line.
static SD_DONE: Signal<CriticalSectionRawMutex, String> = Signal::new();

/// Milliseconds since boot, wrapping as `millis()` does.
#[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
fn millis() -> u32 {
    Instant::now().as_millis() as u32
}

/// Waits until `pin` leaves the level it was sampled at: a level interrupt, so a change between
/// the sample and the wait still ends it at once.
async fn leave(pin: &mut Input<'_>, sampled_low: bool) {
    if sampled_low {
        pin.wait_for_high().await;
    } else {
        pin.wait_for_low().await;
    }
}

#[embassy_executor::task]
async fn buttons(mut up: Input<'static>, mut down: Input<'static>, mut power: Input<'static>) {
    let mut buttons = Buttons::new(POWER_BIT);
    loop {
        let now = millis();
        let held = |input: &Input<'_>, bit: u8| if input.is_low() { bit } else { 0 };
        let sampled = held(&up, UP) | held(&down, DOWN) | held(&power, POWER);
        let change = buttons.update(now, sampled);
        for (bit, count) in PRESSES.iter().enumerate() {
            if change.pressed & (1 << bit) != 0 {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
        let power_down = buttons.state() & POWER != 0;
        POWER_DOWN.store(power_down, Ordering::Relaxed);
        if power_down && buttons.power_held_ms(now) >= LONG_MS {
            LONG.store(true, Ordering::Relaxed);
        }
        // A timer as well when a debounce ends, or Power's hold becomes long.
        let mut timer_ms = buttons.deadline().map(|at| at.wrapping_sub(now));
        if power_down && !LONG.load(Ordering::Relaxed) {
            let left = LONG_MS.saturating_sub(buttons.power_held_ms(now));
            timer_ms = Some(timer_ms.map_or(left, |ms| ms.min(left)));
        }
        let pins = select3(
            leave(&mut up, sampled & UP != 0),
            leave(&mut down, sampled & DOWN != 0),
            leave(&mut power, sampled & POWER != 0),
        );
        let woke = match timer_ms {
            Some(ms) => select(pins, Timer::after_millis(ms.into())).await,
            None => Either::First(pins.await),
        };
        match woke {
            Either::First(_) => PIN_WAKES.fetch_add(1, Ordering::Relaxed),
            Either::Second(()) => TIMER_WAKES.fetch_add(1, Ordering::Relaxed),
        };
    }
}

#[embassy_executor::task]
async fn heartbeat() {
    let mut due = Instant::now();
    loop {
        due += BEAT;
        Timer::at(due).await;
        let late =
            u32::try_from(Instant::now().duration_since(due).as_micros()).unwrap_or(u32::MAX);
        LATEST_US.fetch_max(late, Ordering::Relaxed);
        BEATS.fetch_add(1, Ordering::Relaxed);
    }
}

#[embassy_executor::task]
#[allow(
    clippy::large_futures,
    reason = "the task's future is its static storage, in internal RAM, which the card's DMA needs"
)]
async fn card(card: AsyncCard<'static>, enable: Output<'static>) {
    let line = read_card(card, enable).await;
    SD_DONE.signal(line);
}

/// Mounts the card and reads `IMAGE` whole until `STOP`: the report's line on it.
#[allow(
    clippy::large_futures,
    reason = "as `card`'s, whose future holds this one"
)]
async fn read_card(mut card: AsyncCard<'static>, mut enable: Output<'static>) -> String {
    let Some(attempt) = sd::mount_with(&mut card, &mut enable, &mut Delay, |_| {}).await else {
        return String::from("sd: baglanamadi");
    };
    let mut volume = match Volume::mount(CardBlocks::new(card)).await {
        Ok(v) => v,
        Err(e) => return format!("sd: deneme {attempt}, fat acilamadi ({e:?})"),
    };
    let entry = match volume.find(IMAGE).await {
        Ok(entry) => entry,
        Err(Error::NotFound) => return format!("sd: deneme {attempt}, imaj yok"),
        Err(e) => return format!("sd: deneme {attempt}, imaj bulunamadi ({e:?})"),
    };
    let mut file = match volume.open(&entry) {
        Ok(file) => file,
        Err(e) => return format!("sd: imaj acilamadi ({e:?})"),
    };
    let mut buf = [0u8; sd::RUN * 512];
    let (mut passes, mut differed, mut first) = (0u32, 0u32, None);
    let start = Instant::now();
    while !STOP.load(Ordering::Relaxed) {
        file.seek(0);
        let mut hash = FNV_BASIS;
        loop {
            match volume.read(&mut file, &mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    for &b in buf.get(..n).unwrap_or_default() {
                        hash = (hash ^ u32::from(b)).wrapping_mul(FNV_PRIME);
                    }
                }
                Err(e) => return format!("sd: {passes} geciste okunamadi ({e:?})"),
            }
        }
        passes += 1;
        if *first.get_or_insert(hash) != hash {
            differed += 1;
        }
    }
    let ms = start.elapsed().as_millis().max(1);
    let kb_s = u64::from(passes) * u64::from(file.size()) / ms;
    format!(
        "sd: {passes} gecis, {} KB, {kb_s} KB/s, fnv {:08x}, farkli {differed}",
        file.size() / 1024,
        first.unwrap_or(0)
    )
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal gorev denemesi");
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
    let systimer = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(systimer.alarm0, peripherals.FROM_CPU_INTR0);

    // Active low, with the SDK's internal pull-ups. Left is GPIO0, a boot strap, which only a
    // power-on or system reset samples: the core resets here leave it alone.
    let pulled_up = InputConfig::default().with_pull(Pull::Up);
    let up = Input::new(peripherals.GPIO0, pulled_up);
    let down = Input::new(peripherals.GPIO7, pulled_up);
    let power = Input::new(peripherals.GPIO3, pulled_up);
    spawner.spawn(buttons(up, down, power).unwrap());
    spawner.spawn(heartbeat().unwrap());

    // The card off until its first try.
    let enable = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    // The task's slot borrows the controller, which lives as long as the image, on the heap.
    let slot = SdHostController::new(peripherals.SDHOST, SdConfig::default())
        .ok()
        .map(|controller| &*Box::leak(Box::new(controller)))
        .and_then(|controller| controller.slot::<1>(SlotConfig::default()).ok());
    match slot {
        Some(slot) => {
            let slot = slot
                .with_clk(peripherals.GPIO41)
                .with_cmd(peripherals.GPIO42)
                .with_data0(peripherals.GPIO40)
                .into_async();
            let card_on = AsyncCard::new_uninit(AsyncBus::new(slot), Delay);
            spawner.spawn(self::card(card_on, enable).unwrap());
        }
        None => SD_DONE.signal(String::from("sd: yuva kurulamadi")),
    }
    esp_println::println!("gorevler: basladi");

    let start = Instant::now();
    while start.elapsed() < WINDOW && !LONG.load(Ordering::Relaxed) {
        Timer::after_millis(50).await;
    }
    STOP.store(true, Ordering::Relaxed);
    let sd_line = match select(SD_DONE.wait(), Timer::after(SD_WAIT)).await {
        Either::First(line) => line,
        Either::Second(()) => String::from("sd: bitmedi"),
    };
    let _ = writeln!(report, "{sd_line}");
    let [ups, downs, powers] = PRESSES.each_ref().map(|c| c.load(Ordering::Relaxed));
    let long = if LONG.load(Ordering::Relaxed) {
        "evet"
    } else {
        "yok"
    };
    let _ = writeln!(
        report,
        "dugme: sol {ups} sag {downs} guc {powers} uzun {long}, pin {} sure {}",
        PIN_WAKES.load(Ordering::Relaxed),
        TIMER_WAKES.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        report,
        "kalp: {} vurus, en gec {} us",
        BEATS.load(Ordering::Relaxed),
        LATEST_US.load(Ordering::Relaxed)
    );
    report.save();

    // Power up again before the reset, so the C++ firmware does not start with it held.
    let released = Instant::now();
    while POWER_DOWN.load(Ordering::Relaxed) && released.elapsed() < RELEASE_WAIT {
        Timer::after_millis(10).await;
    }
    if POWER_DOWN.load(Ordering::Relaxed) {
        let _ = writeln!(report, "guc birakilmadi");
    }
    let _ = writeln!(report, "bitti");
    esp_println::println!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
