//! The three buttons for the Embassy main, as the application's events: debounced by
//! `purestwhite-buttons` and Power's hold timed by its `PowerHold`, as `crate::input` does, but
//! woken by the pins' interrupts or a timer instead of polled. While every button is up and no
//! debounce runs, the loop waits on the pins alone, so the chip could sleep lightly (not enabled
//! yet). It ends when asked to rest, before the device sleeps, and hands the pins back (Power then
//! arms the wake).

use crate::input::{ButtonPins, LEFT, POWER, POWER_BIT, POWER_HOLD_MS, RIGHT};
use embassy_futures::select::{Either, select, select3};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::DynamicSender;
use embassy_sync::signal::Signal;
use embassy_time::{Instant, Timer};
use esp_hal::gpio::Input;
use purestwhite_app::{Event, Key};
use purestwhite_buttons::{Buttons, PowerHold, sample_in};

/// The buttons' bits and the keys they are.
const KEYS: [(u8, Key); 3] = [(LEFT, Key::Left), (RIGHT, Key::Right), (POWER, Key::Power)];

/// Milliseconds since boot, wrapping as `millis()` does.
#[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
fn millis() -> u32 {
    Instant::now().as_millis() as u32
}

/// Runs the buttons: each press as `Event::Pressed`, and Power held longer than `POWER_HOLD_MS`
/// (with Right up) as `Event::SleepRequested`, into `events`; until `rest` is set, when it
/// returns the pins.
pub async fn run_keys<'d, M: RawMutex>(
    mut pins: ButtonPins<'d>,
    events: DynamicSender<'_, Event>,
    rest: &Signal<M, ()>,
) -> ButtonPins<'d> {
    let mut buttons = Buttons::new(POWER_BIT);
    let mut hold = PowerHold::new(POWER_HOLD_MS, RIGHT);
    loop {
        let now = millis();
        let sampled = pins.sample();
        let change = buttons.update(now, sampled);
        let sleep = hold.update(&buttons, sampled, now);
        for (bit, key) in KEYS {
            if change.pressed & bit != 0 {
                events.send(Event::Pressed(key)).await;
            }
        }
        if sleep {
            events.send(Event::SleepRequested).await;
        }
        let next = sample_in(&buttons, &hold, now);
        if let Either::Second(()) = select(sample_due(&mut pins, sampled, next), rest.wait()).await
        {
            return pins;
        }
    }
}

/// Waits until a button leaves the level it was sampled at, or `ms` milliseconds when given.
async fn sample_due(pins: &mut ButtonPins<'_>, sampled: u8, ms: Option<u32>) {
    match ms {
        Some(ms) => {
            select(pin_change(pins, sampled), Timer::after_millis(ms.into())).await;
        }
        None => pin_change(pins, sampled).await,
    }
}

/// Waits until any button leaves the level it was sampled at (`sampled`, a bit each, set while
/// down).
async fn pin_change(pins: &mut ButtonPins<'_>, sampled: u8) {
    select3(
        leave(&mut pins.left, sampled & LEFT != 0),
        leave(&mut pins.right, sampled & RIGHT != 0),
        leave(&mut pins.power, sampled & POWER != 0),
    )
    .await;
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
