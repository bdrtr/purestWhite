//! The X4 Pro's input: its three buttons (Left GPIO0, Right GPIO7 and Power GPIO3, the SDK's Up,
//! Down and Power, active low with internal pull-ups) and its GT911 touch panel (on I2C0, SDA 39
//! and SCL 38, shared with the clock and the fuel gauge; RST GPIO4, INT GPIO10, supply GPIO2,
//! active low). [`Input::poll`] reads both once, as the SDK's `InputManager::update()` does, and
//! gives what changed; the caller polls it every `POLL_MS`, from an Embassy task or a plain loop,
//! as nothing in it waits but the I2C transfers.

use embedded_hal_async::i2c::I2c;
use esp_hal::gpio::{Flex, Input as Pin, Level, Output};
use purestwhite_buttons::{Buttons, Change, PowerHold};
use purestwhite_gt911::{Reading, ResetPins};
use purestwhite_touch::{Events, Point, Touch};

/// How often to poll, in milliseconds: the SDK's `beginAsync` default.
pub const POLL_MS: u32 = 15;
/// How long Power must be held to put the device to sleep, in milliseconds: the C++ firmware's
/// `getPowerButtonDuration()` with its default short-press action (10 ms when a short press
/// means sleep). It counts once held longer.
pub const POWER_HOLD_MS: u32 = 400;

/// The buttons' bits in a sampled state.
pub const LEFT: u8 = 1 << 0;
pub const RIGHT: u8 = 1 << 1;
pub const POWER_BIT: u8 = 2;
pub const POWER: u8 = 1 << POWER_BIT;

/// The X4 Pro's touch controller, as freeink-sdk's board profile has it.
pub const TOUCH: purestwhite_gt911::Config = purestwhite_gt911::Config {
    addr: 0x5D,
    alt_addr: 0x14,
    raw_min_x: 0,
    raw_max_x: 799,
    raw_min_y: 0,
    raw_max_y: 479,
    swap_xy: true,
    flip_x: false,
    flip_y: true,
    coords_at_byte0: true,
};

/// The touch controller's RST (GPIO4) and INT (GPIO10), for `purestwhite_gt911::begin`.
pub struct TouchPins<'d> {
    pub reset: Output<'d>,
    pub int: Flex<'d>,
}

impl ResetPins for TouchPins<'_> {
    fn set_reset(&mut self, high: bool) {
        self.reset.set_level(Level::from(high));
    }
    fn drive_int(&mut self, high: bool) {
        // The level first, so the pin starts driving it.
        self.int.set_level(Level::from(high));
        self.int.set_output_enable(true);
    }
    fn release_int(&mut self) {
        self.int.set_output_enable(false);
        self.int.set_input_enable(true);
    }
}

/// The three buttons.
pub struct ButtonPins<'d> {
    pub left: Pin<'d>,
    pub right: Pin<'d>,
    pub power: Pin<'d>,
}

impl ButtonPins<'_> {
    /// Which buttons are down now, one bit each.
    #[must_use]
    pub fn sample(&self) -> u8 {
        let held = |pin: &Pin<'_>, bit: u8| if pin.is_low() { bit } else { 0 };
        held(&self.left, LEFT) | held(&self.right, RIGHT) | held(&self.power, POWER)
    }
}

/// What one poll found.
#[derive(Clone, Copy, Debug, Default)]
pub struct Polled {
    /// The buttons pressed and released, debounced.
    pub buttons: Change,
    /// Power is held longer than the hold time `Input::new` took, with Right up (Power with Right
    /// is the C++ firmware's screenshot), and Power was up once since the start: the C++
    /// firmware's request to sleep (`purestwhite_buttons::PowerHold`). Once per press.
    pub sleep: bool,
    /// What the touch panel's reading ended; nothing when there is no panel.
    pub touch: Events,
    /// The touch panel's status or contacts could not be read (the contact is kept as it was).
    pub touch_failed: bool,
    /// While a single contact is down, where it is now (`Touch::held_at`), for dragging.
    pub held: Option<Point>,
    /// While a single contact is down and still in place, where it touched down and for how long
    /// (`Touch::tap_candidate`), for pressed feedback.
    pub candidate: Option<(Point, u32)>,
}

impl Polled {
    /// The buttons' and touch panel's part of what ends the C++ firmware's idle wait: a button
    /// pressed or released, or touch activity.
    #[must_use]
    pub fn activity(&self) -> bool {
        !self.buttons.is_empty() || self.touch.activity
    }
}

/// The buttons and the touch panel, with their state between polls.
pub struct Input<'d, B> {
    pins: ButtonPins<'d>,
    /// The touch panel's bus and address, when it was found.
    touch: Option<(B, u8)>,
    buttons: Buttons,
    gestures: Touch,
    power_hold: PowerHold,
}

impl<'d, B: I2c> Input<'d, B> {
    /// The buttons, and the touch panel on `bus` at `addr` (what `purestwhite_gt911::begin`
    /// found) when there is one. Power held longer than `power_hold_ms` asks to sleep
    /// (`POWER_HOLD_MS` by default).
    pub fn new(pins: ButtonPins<'d>, touch: Option<(B, u8)>, power_hold_ms: u32) -> Self {
        Input {
            pins,
            touch,
            buttons: Buttons::new(POWER_BIT),
            gestures: Touch::new(),
            power_hold: PowerHold::new(power_hold_ms, RIGHT),
        }
    }

    /// Reads the buttons and the touch panel at time `now`, in milliseconds.
    pub async fn poll(&mut self, now: u32) -> Polled {
        let sampled = self.pins.sample();
        let buttons = self.buttons.update(now, sampled);
        let sleep = self.power_hold.update(&self.buttons, sampled, now);
        let (touch, touch_failed) = match self.touch.as_mut() {
            Some((bus, addr)) => {
                let reading = purestwhite_gt911::read(bus, *addr, &TOUCH).await;
                let failed = matches!(reading, Reading::Failed | Reading::ContactsFailed { .. });
                (self.gestures.update(now, &reading), failed)
            }
            None => (Events::default(), false),
        };
        Polled {
            buttons,
            sleep,
            touch,
            touch_failed,
            held: self.gestures.held_at(),
            candidate: self.gestures.tap_candidate(now),
        }
    }

    /// Ignores the rest of the current touch contact (`purestwhite_touch::Touch::suppress`).
    pub fn suppress_touch(&mut self) {
        self.gestures.suppress();
    }

    /// How long the last touch contact lasted, from touch-down to lift.
    #[must_use]
    pub fn last_touch_held_ms(&self) -> u32 {
        self.gestures.last_held_ms()
    }

    /// Whether Power is down, debounced.
    #[must_use]
    pub fn power_down(&self) -> bool {
        self.buttons.state() & POWER != 0
    }

    /// The buttons, back, for the sleep that follows.
    pub fn into_pins(self) -> ButtonPins<'d> {
        self.pins
    }
}

/// A blocking I2C bus as embedded-hal-async's: every transfer runs to its end at once. For
/// images without an executor, which drive the async drivers with `crate::ready`.
pub struct ReadyI2c<T>(pub T);

impl<T: embedded_hal::i2c::ErrorType> embedded_hal_async::i2c::ErrorType for ReadyI2c<T> {
    type Error = T::Error;
}

#[allow(clippy::unused_async, reason = "blocking calls, done at once")]
impl<T: embedded_hal::i2c::I2c> I2c for ReadyI2c<T> {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        self.0.transaction(address, operations)
    }
}
