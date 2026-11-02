//! The image's power side, run by `main` once the tasks are started: the watchdog fed while the
//! executor runs, then, once the application asks to sleep, the sleep handshake (DESIGN.md,
//! Sleep): the panel rests and hands back its reset line, the keys rest and hand back their pins,
//! the card and the gauge rest (the card hands back its supply), Power is let go, and the chip
//! sleeps deeply until Power. During phase 2 the wake boots the C++ firmware, which `boot::start`
//! selected, and the sleep is bounded (`boot::BoundedSleep`): it ends after `SLEEP.timer` at most,
//! and the watchdog counts through it.

use crate::battery::{GAUGE_RESTED, Measurement};
use crate::card::{self, CARD_RESTED};
use crate::tasks::{KEYS_RESTED, LINKS, PANEL_LIMIT, PANEL_RESTED, PanelRested, SHOWN};
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Output;
use esp_hal::peripherals::LPWR;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::time::Duration as HalDuration;
use purestwhite_app::SleepReason;
use purestwhite_firmware::boot::{self, BoundedSleep, Report};
use purestwhite_firmware::sleep::Rails;

/// How often the watchdog (60 s) is fed while the executor runs.
pub const FEED: Duration = Duration::from_secs(10);
/// How long the image runs, a frame shown, before the run counts as healthy (the boot counter,
/// rust/firmware/DESIGN.md); with a frame shown, reaching the orderly sleep counts too.
const HEALTHY_AFTER: Duration = Duration::from_secs(30);
/// Set by the power loop once it begins the sleep.
pub static SLEEPING: AtomicBool = AtomicBool::new(false);
/// Set by the phase-2 backstop (`crate::backstop`) once it has asked for the sleep: the
/// application sees it as Power held.
pub static TIME_UP: AtomicBool = AtomicBool::new(false);
/// How long a loop may take to rest before a reset: the panel's within `PANEL_LIMIT`.
const REST_MOST: Duration = Duration::from_secs(2 * PANEL_LIMIT.as_secs() + FEED.as_secs());
/// The deep sleep: until Power or for 10 minutes at most; if the chip has not woken 2 minutes after
/// that, the watchdog resets its core, and the whole system 30 s after that. Power may stay down
/// 10 s before it starts anyway (a wake pin at its wake level would end it at once).
const SLEEP: BoundedSleep = BoundedSleep {
    timer: HalDuration::from_secs(10 * 60),
    core: HalDuration::from_secs(12 * 60),
    system: HalDuration::from_secs(30),
    release: HalDuration::from_secs(10),
};

/// What the power side owns: the watchdog, the deep sleep, the touch panel's supply (on for the
/// shared I2C bus), the report and its shape during a measurement.
pub struct Power {
    rtc: Rtc<'static>,
    lpwr: LPWR<'static>,
    touch_supply: Output<'static>,
    report: Report,
    measurement: Measurement,
    /// Whether the run is marked healthy.
    healthy: bool,
    /// Whether the card's line is in the report.
    card_noted: bool,
}

impl Power {
    /// `touch_supply` is on (active low) for the gauge's bus; `measurement` the report's shape
    /// should a measurement begin.
    pub fn new(
        rtc: Rtc<'static>,
        lpwr: LPWR<'static>,
        touch_supply: Output<'static>,
        report: Report,
        measurement: Measurement,
    ) -> Self {
        Power {
            rtc,
            lpwr,
            touch_supply,
            report,
            measurement,
            healthy: false,
            card_noted: false,
        }
    }

    /// Feeds the watchdog until the application asks to sleep, marking the run healthy and
    /// noting the card and the battery on the way, then sleeps.
    pub async fn run(mut self) -> ! {
        let reason = self.watch().await;
        SLEEPING.store(true, Ordering::Release);
        let why = sleep_why(reason);
        LINKS.panel_rest.signal(());
        let panel = self.within(PANEL_RESTED.wait(), "panel did not rest").await;
        LINKS.keys_rest.signal(());
        let pins = self.within(KEYS_RESTED.wait(), "keys did not rest").await;
        LINKS.storage_rest.signal(());
        let sd_supply = self.within(CARD_RESTED.wait(), "card did not rest").await;
        LINKS.gauge_rest.signal(());
        self.within(GAUGE_RESTED.wait(), "gauge did not rest").await;
        // The orderly sleep is reached, every loop at rest: healthy, once a frame was shown.
        if !self.healthy && SHOWN.load(Ordering::Acquire) {
            self.mark_healthy("sleep");
        }
        self.measurement.note(&mut self.report);
        self.note_panel(&panel);
        let rails = Rails {
            touch_power: self.touch_supply,
            sd_power: sd_supply,
            panel_reset: panel.reset,
        };
        let low_power = LowPower::new(self.lpwr);
        let (report, rtc) = (&mut self.report, &mut self.rtc);
        // Blocking from here (the wait for Power, then the sleep): only as the last step, with
        // every other task at rest.
        boot::sleep_when_released(report, why, rtc, pins.power, rails, low_power, SLEEP)
    }

    /// Until the application asks to sleep: the watchdog fed every `FEED`, the run marked healthy
    /// after `HEALTHY_AFTER` with a frame shown, the card's line once it is known (outside a
    /// measurement) and the battery line during one; why the application asked.
    async fn watch(&mut self) -> SleepReason {
        let started = Instant::now();
        loop {
            if let Either::First(reason) = select(LINKS.sleep.wait(), Timer::after(FEED)).await {
                return reason;
            }
            self.rtc.rwdt.feed();
            if !self.healthy && SHOWN.load(Ordering::Acquire) && started.elapsed() >= HEALTHY_AFTER
            {
                self.mark_healthy("30 s");
            }
            if !self.card_noted && !Measurement::begun() && card::line(&mut self.report) {
                self.report.save();
                self.card_noted = true;
            }
            self.measurement.note(&mut self.report);
        }
    }

    /// Marks the run healthy for the boot counter and, outside a measurement (whose report has
    /// no room for it), notes `healthy: <why>` in the report.
    fn mark_healthy(&mut self, why: &str) {
        boot::finish_run();
        self.healthy = true;
        if !Measurement::begun() {
            let _ = writeln!(self.report, "healthy: {why}");
            self.report.save();
        }
    }

    /// `wait`'s result, feeding the watchdog meanwhile; after `REST_MOST`, `what` noted and a
    /// reset.
    async fn within<T>(&mut self, wait: impl Future<Output = T>, what: &str) -> T {
        let until = Instant::now() + REST_MOST;
        let mut wait = core::pin::pin!(wait);
        while Instant::now() < until {
            match select(wait.as_mut(), Timer::after(FEED)).await {
                Either::First(value) => return value,
                Either::Second(()) => self.rtc.rwdt.feed(),
            }
        }
        let _ = writeln!(self.report, "{what}");
        self.report.save();
        esp_hal::system::software_reset()
    }

    /// The panel's line: its calls, and whether it went to sleep.
    fn note_panel(&mut self, panel: &PanelRested) {
        let s = panel.stats;
        let _ = writeln!(
            self.report,
            "panel: {} ok, {} failed, {} given up, longest {} ms, {}",
            s.ok,
            s.failed,
            s.given_up(),
            s.longest_ms,
            if panel.slept { "asleep" } else { "NOT asleep" },
        );
        self.report.save();
    }
}

/// The sleep line's reason: `power` (held), `idle` (no key for ten minutes), `hour` (the page
/// loop's end), or `time up` (the phase-2 backstop's).
fn sleep_why(reason: SleepReason) -> &'static str {
    if TIME_UP.load(Ordering::Acquire) {
        return "time up";
    }
    match reason {
        SleepReason::Power => "power",
        SleepReason::Idle => "idle",
        SleepReason::LoopDone => "hour",
    }
}
