//! The X4 Pro's panel for the test images: its lines, the controller probe, and the driver for
//! the controller found, run through blocking SPI and delays whose futures are ready at once.

use crate::board::PanelParts;
use crate::ready;
use esp_hal::delay::Delay;
use esp_hal::gpio::{AnyPin, Flex, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::{Duration, Instant, Rate};
use purestwhite_epd::probe::{self, Controller, Line, Mode, Probe, ProbeLines};
use purestwhite_epd::uc8179::{GrayMode, Uc8179, X4_PRO as UC8179_X4_PRO};
use purestwhite_epd::uc8279x4::{Refresh, Uc8279X4, X4_PRO};
use purestwhite_epd::{Bus, BusError, Interface};

/// How long a driver call may take. A full refresh takes a few seconds; past this BUSY is taken
/// as stuck, so the test goes on instead of waiting for the watchdog.
const CALL_LIMIT: Duration = Duration::from_secs(8);

/// Blocking SPI as embedded-hal-async's: each future is ready at once.
pub struct BlockingSpi<S>(pub S);

impl<S: embedded_hal::spi::SpiBus> embedded_hal_async::spi::ErrorType for BlockingSpi<S> {
    type Error = S::Error;
}

#[allow(clippy::unused_async, reason = "blocking calls, ready at once")]
impl<S: embedded_hal::spi::SpiBus> embedded_hal_async::spi::SpiBus for BlockingSpi<S> {
    async fn read(&mut self, words: &mut [u8]) -> Result<(), S::Error> {
        self.0.read(words)
    }
    async fn write(&mut self, words: &[u8]) -> Result<(), S::Error> {
        self.0.write(words)
    }
    async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), S::Error> {
        self.0.transfer(read, write)
    }
    async fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), S::Error> {
        self.0.transfer_in_place(words)
    }
    async fn flush(&mut self) -> Result<(), S::Error> {
        self.0.flush()
    }
}

/// esp-hal's busy-wait delay as embedded-hal-async's. Past the deadline a delay never ends, so
/// `ready` gives the call up.
pub struct BlockingDelay {
    delay: Delay,
    deadline: Option<Instant>,
    /// No call's deadline is later (see `finish_by`).
    last: Option<Instant>,
}

impl BlockingDelay {
    fn new() -> Self {
        BlockingDelay {
            delay: Delay::new(),
            deadline: None,
            last: None,
        }
    }
}

impl embedded_hal_async::delay::DelayNs for BlockingDelay {
    async fn delay_ns(&mut self, ns: u32) {
        if self.deadline.is_some_and(|d| Instant::now() > d) {
            core::future::pending::<()>().await;
        }
        embedded_hal::delay::DelayNs::delay_ns(&mut self.delay, ns);
    }
}

/// The panel's lines as GPIOs for the probe.
struct PanelGpio<'d> {
    sclk: Flex<'d>,
    sda: Flex<'d>,
    cs: Flex<'d>,
    dc: Flex<'d>,
    rst: Flex<'d>,
    busy: Flex<'d>,
}

impl<'d> PanelGpio<'d> {
    fn pin(&mut self, line: Line) -> &mut Flex<'d> {
        match line {
            Line::Sclk => &mut self.sclk,
            Line::Sda => &mut self.sda,
            Line::Cs => &mut self.cs,
            Line::Dc => &mut self.dc,
            Line::Rst => &mut self.rst,
            Line::Busy => &mut self.busy,
        }
    }
}

impl ProbeLines for PanelGpio<'_> {
    fn mode(&mut self, line: Line, mode: Mode) {
        let pin = self.pin(line);
        match mode {
            Mode::Output => {
                // As Arduino's pinMode(OUTPUT): no pull left from an input setting.
                pin.apply_input_config(&InputConfig::default());
                pin.set_input_enable(false);
                pin.set_output_enable(true);
            }
            Mode::Input | Mode::InputPullUp => {
                pin.set_output_enable(false);
                let pull = if mode == Mode::Input {
                    Pull::None
                } else {
                    Pull::Up
                };
                pin.apply_input_config(&InputConfig::default().with_pull(pull));
                pin.set_input_enable(true);
            }
        }
    }
    fn write(&mut self, line: Line, high: bool) {
        self.pin(line).set_level(Level::from(high));
    }
    fn read(&mut self, line: Line) -> bool {
        self.pin(line).is_high()
    }
    /// Taking a pin (`Flex::new`) already released any pad hold.
    fn release_reset_hold(&mut self) {}
}

/// A panel line as a push-pull GPIO, for the probe.
fn flex(pin: AnyPin<'_>) -> Flex<'_> {
    let mut f = Flex::new(pin);
    f.apply_output_config(&OutputConfig::default());
    f
}

impl PanelParts {
    /// Which controller drives the panel, with the lines as GPIOs for the probe's moment: the
    /// bytes the C++ firmware's probe sends at every boot. `None` if the probe could not run.
    fn probe(&mut self) -> Option<Probe> {
        let mut gpio = PanelGpio {
            sclk: flex(self.sclk.reborrow().into()),
            sda: flex(self.mosi.reborrow().into()),
            cs: flex(self.cs.reborrow().into()),
            dc: flex(self.dc.reborrow().into()),
            rst: flex(self.rst.reborrow().into()),
            busy: flex(self.busy.reborrow().into()),
        };
        ready(probe::probe(&mut gpio, &mut BlockingDelay::new()))
    }
}

/// The lines as the test images drive them.
pub type Lines<'d> = Bus<
    BlockingSpi<Spi<'d, esp_hal::Blocking>>,
    Output<'d>,
    Output<'d>,
    Output<'d>,
    Input<'d>,
    BlockingDelay,
>;

impl PanelParts {
    /// The lines as SPI at 10 MHz, mode 0, with CS, DC and RST high and BUSY an input without a
    /// pull; `None` if SPI cannot be set up.
    fn into_lines(self) -> Option<Lines<'static>> {
        let spi = Spi::new(
            self.spi,
            SpiConfig::default().with_frequency(Rate::from_mhz(10)),
        )
        .ok()?
        .with_sck(self.sclk)
        .with_mosi(self.mosi);
        Some(Bus {
            spi: BlockingSpi(spi),
            cs: Output::new(self.cs, Level::High, OutputConfig::default()),
            dc: Output::new(self.dc, Level::High, OutputConfig::default()),
            rst: Output::new(self.rst, Level::High, OutputConfig::default()),
            busy: Input::new(self.busy, InputConfig::default()),
            delay: BlockingDelay::new(),
        })
    }
}

/// The panel, ready to begin: what the probe found, the driver for it, and the lines.
pub struct Opened {
    pub probe: Probe,
    pub panel: Panel,
    pub lines: Lines<'static>,
}

/// Why the panel cannot be used, with what the probe found when it ran.
pub enum OpenError {
    /// The probe could not run.
    Probe,
    /// No driver here for the controller found (the SSD1677): nothing more goes to it.
    NoDriver(Probe),
    /// SPI could not be set up.
    Spi(Probe),
}

impl OpenError {
    /// What the probe found, when it ran.
    #[must_use]
    pub fn probe(&self) -> Option<&Probe> {
        match self {
            OpenError::Probe => None,
            OpenError::NoDriver(found) | OpenError::Spi(found) => Some(found),
        }
    }
}

/// The panel: its controller probed with the lines as GPIOs for a moment, as the C++ firmware
/// probes at every boot; the driver for it (for the UC8179, with the grayscale base that `base`
/// returns for it; see `Panel::for_controller`); and the lines as SPI. The driver is not begun.
///
/// # Errors
/// The step that failed (`OpenError`).
pub fn open(
    mut parts: PanelParts,
    base: impl FnOnce(Controller) -> Option<&'static mut [u8]>,
) -> Result<Opened, OpenError> {
    let probe = parts.probe().ok_or(OpenError::Probe)?;
    let controller = probe.controller();
    let Some(panel) = Panel::for_controller(controller, base(controller)) else {
        return Err(OpenError::NoDriver(probe));
    };
    let lines = parts.into_lines().ok_or(OpenError::Spi(probe))?;
    Ok(Opened {
        probe,
        panel,
        lines,
    })
}

/// The panel driver for the controller found.
pub enum Panel {
    Uc8279(Uc8279X4),
    Uc8179(Uc8179),
}

/// Gives the next driver call `CALL_LIMIT` to finish, and no later than `finish_by`'s time;
/// `false`, and the call is not to start, once that time has passed.
fn limit(bus: &mut Lines<'_>) -> bool {
    let now = Instant::now();
    if bus.delay.last.is_some_and(|last| now > last) {
        return false;
    }
    let call = now + CALL_LIMIT;
    bus.delay.deadline = Some(bus.delay.last.map_or(call, |last| call.min(last)));
    true
}

/// Once `by` has passed no driver call starts, and one under way stops at its next wait: a run
/// of them ends before the watchdog however slowly each one finishes.
pub fn finish_by(bus: &mut Lines<'_>, by: Instant) {
    bus.delay.last = Some(by);
}

/// Lifts the call's deadline; whether the call went through.
fn settle(bus: &mut Lines<'_>, done: Option<Result<(), BusError>>) -> bool {
    bus.delay.deadline = None;
    matches!(done, Some(Ok(())))
}

impl Panel {
    /// `None` for a controller without a driver here (the SSD1677). The UC8179 keeps its
    /// grayscale base in `base` when given one a frame long.
    fn for_controller(controller: Controller, base: Option<&'static mut [u8]>) -> Option<Self> {
        match controller {
            Controller::Uc8279(_) => Some(Panel::Uc8279(Uc8279X4::new(X4_PRO))),
            Controller::Uc8179(_) => Some(Panel::Uc8179(
                base.and_then(|b| Uc8179::with_base(UC8179_X4_PRO, b))
                    .unwrap_or_else(|| Uc8179::new(UC8179_X4_PRO)),
            )),
            Controller::Ssd1677 => None,
        }
    }

    /// Whether the call went through: no bus error, and done within `CALL_LIMIT`. After one that
    /// did not, the controller's state is unknown and nothing more should go to it.
    pub fn begin(&mut self, bus: &mut Lines<'_>) -> bool {
        if !limit(bus) {
            return false;
        }
        let done = ready(self.begin_on(bus));
        settle(bus, done)
    }

    /// As `begin`; `frame` has a bit per pixel, 1 white, 100-byte rows, 480 of them.
    pub fn display(
        &mut self,
        bus: &mut Lines<'_>,
        frame: &[u8],
        refresh: Refresh,
        power_off: bool,
    ) -> bool {
        if !limit(bus) {
            return false;
        }
        let done = ready(self.display_on(bus, frame, refresh, power_off));
        settle(bus, done)
    }

    /// As `begin`.
    pub fn deep_sleep(&mut self, bus: &mut Lines<'_>) -> bool {
        if !limit(bus) {
            return false;
        }
        let done = ready(self.deep_sleep_on(bus));
        settle(bus, done)
    }

    /// The driver's `begin` over any of the panel's line sets: the blocking calls above run it
    /// with `ready`, the Embassy main's `crate::screen` awaits it.
    ///
    /// # Errors
    /// The driver's.
    pub async fn begin_on(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        match self {
            Panel::Uc8279(p) => p.begin(bus).await,
            Panel::Uc8179(p) => p.begin(bus).await,
        }
    }

    /// As `begin_on`, for `display`.
    ///
    /// # Errors
    /// The driver's.
    pub async fn display_on(
        &mut self,
        bus: &mut impl Interface,
        frame: &[u8],
        refresh: Refresh,
        power_off: bool,
    ) -> Result<(), BusError> {
        match self {
            Panel::Uc8279(p) => p.display(bus, frame, refresh, power_off).await,
            Panel::Uc8179(p) => p.display(bus, frame, refresh, power_off).await,
        }
    }

    /// As `begin_on`, for `deep_sleep`.
    ///
    /// # Errors
    /// The driver's.
    pub async fn deep_sleep_on(&mut self, bus: &mut impl Interface) -> Result<(), BusError> {
        match self {
            Panel::Uc8279(p) => p.deep_sleep(bus).await,
            Panel::Uc8179(p) => p.deep_sleep(bus).await,
        }
    }

    /// Whether the UC8179 keeps its grayscale base (`for_controller` was given one it took).
    #[must_use]
    pub fn has_gray_base(&self) -> bool {
        matches!(self, Panel::Uc8179(p) if p.has_base())
    }

    /// A grayscale pass on the UC8179, as CrossPoint runs one; `false` on the UC8279, which has
    /// none here. `fill(frame, plane)` writes into `frame` the base (`None`; `Direct` does not
    /// show it) and the LSB and MSB planes (`Some(false)`, `Some(true)`) in the encoding of `mode`
    /// (see [`GrayMode`]). Overlay is the reader's page: its base is a `Fast` refresh and the
    /// planes get the base back after the gray refresh. The others are an image's: the base is a
    /// `Half` refresh and a resync follows. Each step as `begin`.
    pub fn gray(
        &mut self,
        bus: &mut Lines<'_>,
        frame: &mut [u8],
        mode: GrayMode,
        fill: impl Fn(&mut [u8], Option<bool>),
    ) -> bool {
        let Panel::Uc8179(p) = self else {
            return false;
        };
        let reader = mode == GrayMode::Overlay;
        fill(frame, None);
        if !limit(bus) {
            return false;
        }
        let fallback = if reader { Refresh::Fast } else { Refresh::Half };
        let done = ready(p.begin_grayscale(bus, frame, mode, fallback, false));
        if !settle(bus, done) {
            return false;
        }
        for msb in [false, true] {
            fill(frame, Some(msb));
            if !limit(bus) {
                return false;
            }
            let done = if msb {
                ready(p.copy_grayscale_msb(bus, frame))
            } else {
                ready(p.copy_grayscale_lsb(bus, frame))
            };
            if !settle(bus, done) {
                return false;
            }
        }
        if !limit(bus) {
            return false;
        }
        let done = ready(p.display_gray(bus, false, !reader));
        if !settle(bus, done) {
            return false;
        }
        if !reader {
            p.request_resync();
            return true;
        }
        fill(frame, None);
        if !limit(bus) {
            return false;
        }
        let done = ready(p.cleanup_grayscale(bus, Some(frame)));
        settle(bus, done)
    }
}
