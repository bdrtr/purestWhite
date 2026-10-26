//! The X4 Pro's panel for the test images: its lines, the controller probe, and the driver for
//! the controller found, run through blocking SPI and delays whose futures are ready at once.

use core::pin::pin;
use core::task::{Context, Poll, Waker};
use esp_hal::delay::Delay;
use esp_hal::gpio::{AnyPin, Flex, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::peripherals::{GPIO6, GPIO11, GPIO12, GPIO13, GPIO14, GPIO18, SPI2};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::{Duration, Instant, Rate};
use purestwhite_epd::probe::{self, Controller, Line, Mode, ProbeLines};
use purestwhite_epd::uc8179::{Uc8179, X4_PRO as UC8179_X4_PRO};
use purestwhite_epd::uc8279x4::{Refresh, Uc8279X4, X4_PRO};
use purestwhite_epd::{Bus, BusError};

/// How long a driver call may take. A full refresh takes a few seconds; past this BUSY is taken
/// as stuck, so the test goes on instead of waiting for the watchdog.
const CALL_LIMIT: Duration = Duration::from_secs(8);

/// Runs a future whose awaits are all ready at once; `None` if one is not.
fn ready<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

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
}

impl BlockingDelay {
    fn new() -> Self {
        BlockingDelay {
            delay: Delay::new(),
            deadline: None,
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

/// The panel's peripherals (freeink-sdk's board profile): SCLK 12, MOSI 11, CS 13, DC 18,
/// RST 14, BUSY 6, on SPI2.
pub struct PanelPins<'d> {
    pub spi: SPI2<'d>,
    pub sclk: GPIO12<'d>,
    pub mosi: GPIO11<'d>,
    pub cs: GPIO13<'d>,
    pub dc: GPIO18<'d>,
    pub rst: GPIO14<'d>,
    pub busy: GPIO6<'d>,
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

impl PanelPins<'_> {
    /// Which controller drives the panel, with the lines as GPIOs for the probe's moment: the
    /// bytes the C++ firmware's probe sends at every boot. `None` if the probe could not run.
    pub fn probe(&mut self) -> Option<probe::Probe> {
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

impl<'d> PanelPins<'d> {
    /// The lines as SPI at 10 MHz, mode 0, with CS, DC and RST high and BUSY an input without a
    /// pull; `None` if SPI cannot be set up.
    #[must_use]
    pub fn open(self) -> Option<Lines<'d>> {
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

/// The panel driver for the controller found.
pub enum Panel {
    Uc8279(Uc8279X4),
    Uc8179(Uc8179),
}

/// Gives the next driver call `CALL_LIMIT` to finish.
fn limit(bus: &mut Lines<'_>) {
    bus.delay.deadline = Some(Instant::now() + CALL_LIMIT);
}

/// Lifts the call's deadline; whether the call went through.
fn settle(bus: &mut Lines<'_>, done: Option<Result<(), BusError>>) -> bool {
    bus.delay.deadline = None;
    matches!(done, Some(Ok(())))
}

impl Panel {
    /// `None` for a controller without a driver here (the SSD1677).
    #[must_use]
    pub fn for_controller(controller: Controller) -> Option<Self> {
        match controller {
            Controller::Uc8279(_) => Some(Panel::Uc8279(Uc8279X4::new(X4_PRO))),
            Controller::Uc8179(_) => Some(Panel::Uc8179(Uc8179::new(UC8179_X4_PRO))),
            Controller::Ssd1677 => None,
        }
    }

    /// Whether the call went through: no bus error, and done within `CALL_LIMIT`. After one that
    /// did not, the controller's state is unknown and nothing more should go to it.
    pub fn begin(&mut self, bus: &mut Lines<'_>) -> bool {
        limit(bus);
        let done = match self {
            Panel::Uc8279(p) => ready(p.begin(bus)),
            Panel::Uc8179(p) => ready(p.begin(bus)),
        };
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
        limit(bus);
        let done = match self {
            Panel::Uc8279(p) => ready(p.display(bus, frame, refresh, power_off)),
            Panel::Uc8179(p) => ready(p.display(bus, frame, refresh, power_off)),
        };
        settle(bus, done)
    }

    /// As `begin`.
    pub fn deep_sleep(&mut self, bus: &mut Lines<'_>) -> bool {
        limit(bus);
        let done = match self {
            Panel::Uc8279(p) => ready(p.deep_sleep(bus)),
            Panel::Uc8179(p) => ready(p.deep_sleep(bus)),
        };
        settle(bus, done)
    }
}
