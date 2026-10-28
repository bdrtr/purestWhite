//! The panel as the services' `Screen`, for the Embassy main: the probed controller's driver
//! (`crate::panel::Panel`) over the panel's lines, with its delays on embassy-time's timer and its
//! waits for BUSY on the pin's interrupt, so the other tasks run while the panel refreshes. SPI
//! stays blocking: a 48,000-byte plane takes about 40 ms at 10 MHz.

use crate::panel::{BlockingSpi, Lines, Opened, Panel};
use embassy_futures::select::select;
use embassy_time::{Delay, Instant, Timer};
use esp_hal::Blocking;
use esp_hal::gpio::{Input, Output};
use esp_hal::spi::master::Spi;
use purestwhite_app::Refresh;
use purestwhite_epd::uc8279x4::Refresh as PanelRefresh;
use purestwhite_epd::{Bus, BusError, Interface};
use purestwhite_render::FRAME_BYTES;
use purestwhite_services::{Screen, ScreenError};

/// The panel's lines with embassy-time's delays.
type TimerBus = Bus<
    BlockingSpi<Spi<'static, Blocking>>,
    Output<'static>,
    Output<'static>,
    Output<'static>,
    Input<'static>,
    Delay,
>;

/// The panel's lines for the drivers: as `TimerBus`, but BUSY is awaited on its interrupt
/// instead of read every millisecond.
pub struct AsyncLines(TimerBus);

impl Interface for AsyncLines {
    async fn reset(&mut self, settle_ms: u32) -> Result<(), BusError> {
        self.0.reset(settle_ms).await
    }

    async fn cmd(&mut self, command: u8) -> Result<(), BusError> {
        self.0.cmd(command).await
    }

    async fn data(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        self.0.data(bytes).await
    }

    async fn frame(&mut self, command: u8, bytes: &[u8]) -> Result<(), BusError> {
        self.0.frame(command, bytes).await
    }

    async fn burst(
        &mut self,
        command: u8,
        row: &mut [u8],
        rows: u16,
        fill: impl FnMut(u16, &mut [u8]),
    ) -> Result<(), BusError> {
        self.0.burst(command, row, rows, fill).await
    }

    /// As `Bus`'s: one tick for the command to take effect, then until BUSY (active low) is
    /// high, with no time limit of its own (the panel loop's applies).
    async fn wait_idle(&mut self) -> Result<(), BusError> {
        Timer::after_millis(1).await;
        self.0.busy.wait_for_high().await;
        Ok(())
    }

    async fn wait_busy_start(&mut self, max_ms: u32) -> Result<(), BusError> {
        select(
            self.0.busy.wait_for_low(),
            Timer::after_millis(max_ms.into()),
        )
        .await;
        Ok(())
    }
}

impl From<Lines<'static>> for AsyncLines {
    fn from(lines: Lines<'static>) -> Self {
        let Bus {
            spi,
            cs,
            dc,
            rst,
            busy,
            delay: _,
        } = lines;
        AsyncLines(Bus {
            spi,
            cs,
            dc,
            rst,
            busy,
            delay: Delay,
        })
    }
}

/// What the panel did, for the report: calls started, ended well and failed (the rest were given
/// up by the panel loop), and the longest frame shown, in milliseconds. Many given up, or frames
/// about as long as the panel loop's limit, mean BUSY's interrupt did not wake the task.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScreenStats {
    pub started: u32,
    pub ok: u32,
    pub failed: u32,
    pub longest_ms: u64,
}

impl ScreenStats {
    /// Calls the panel loop gave up on.
    #[must_use]
    pub fn given_up(&self) -> u32 {
        self.started.saturating_sub(self.ok + self.failed)
    }
}

/// The panel as a `Screen`: begun before its first frame, and again after any call that failed
/// or was given up on (its state is unknown then).
pub struct DeviceScreen {
    driver: Panel,
    lines: AsyncLines,
    begun: bool,
    /// The controller's name, for the report.
    pub name: &'static str,
    pub stats: ScreenStats,
}

impl DeviceScreen {
    /// The panel `panel::open` opened, not yet begun.
    #[must_use]
    pub fn new(opened: Opened) -> Self {
        DeviceScreen {
            name: opened.probe.controller().name(),
            driver: opened.panel,
            lines: opened.lines.into(),
            begun: false,
            stats: ScreenStats::default(),
        }
    }

    /// The panel's reset line, which the deep sleep holds high, once the panel is asleep.
    #[must_use]
    pub fn into_reset(self) -> Output<'static> {
        self.lines.0.rst
    }

    /// Begins the panel when it is not known to be begun. Until the call that follows succeeds,
    /// it is taken as not begun, so that a call dropped halfway begins it again.
    async fn begin_if_needed(&mut self) -> Result<(), BusError> {
        if core::mem::replace(&mut self.begun, false) {
            return Ok(());
        }
        self.driver.begin_on(&mut self.lines).await
    }

    /// `result`, counted.
    fn count(&mut self, result: Result<(), BusError>) -> Result<(), ScreenError> {
        if result.is_ok() {
            self.stats.ok += 1;
        } else {
            self.stats.failed += 1;
        }
        result.map_err(|_| ScreenError)
    }

    async fn show_frame(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), BusError> {
        self.begin_if_needed().await?;
        let refresh = match refresh {
            Refresh::Full => PanelRefresh::Full,
            Refresh::Half => PanelRefresh::Half,
            Refresh::Fast => PanelRefresh::Fast,
        };
        // Left on between refreshes, as the C++ firmware leaves it (`turnOffScreen` false).
        self.driver
            .display_on(&mut self.lines, frame, refresh, false)
            .await?;
        self.begun = true;
        Ok(())
    }
}

impl Screen for DeviceScreen {
    async fn show(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), ScreenError> {
        self.stats.started += 1;
        let start = Instant::now();
        let shown = self.show_frame(frame, refresh).await;
        self.stats.longest_ms = self.stats.longest_ms.max(start.elapsed().as_millis());
        self.count(shown)
    }

    async fn sleep(&mut self) -> Result<(), ScreenError> {
        self.stats.started += 1;
        let slept = match self.begin_if_needed().await {
            Ok(()) => self.driver.deep_sleep_on(&mut self.lines).await,
            Err(e) => Err(e),
        };
        self.count(slept)
    }
}
