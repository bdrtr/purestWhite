//! Up to eight push buttons, debounced as freeink-sdk's `InputManager` debounces plain GPIO
//! buttons: a sampled state that has not changed for more than `DEBOUNCE_MS` replaces the
//! committed one, and the commit reports the buttons pressed and released and keeps when a press
//! began and ended. Times are milliseconds from any start, and wrap as the SDK's `millis()` does.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests;

/// How long a sampled state must stay the same before it counts, in milliseconds: the SDK's
/// `DEBOUNCE_DELAY`. The state counts once more than this has passed.
pub const DEBOUNCE_MS: u32 = 5;

/// The buttons that one commit pressed and released, one bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub pressed: u8,
    pub released: u8,
}

impl Change {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pressed == 0 && self.released == 0
    }
}

/// The buttons' debounced state and press times. Bit `i` of a state is button `i`, set while it
/// is pressed; one of the buttons is the power button, whose press is timed on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buttons {
    power: u8,
    /// The last state sampled, and when it last changed.
    sampled: u8,
    sampled_at: u32,
    /// The committed state.
    state: u8,
    /// When the first button of a press went down and the last came up.
    press_start: u32,
    press_finish: u32,
    /// The same for the power button.
    power_start: u32,
    power_finish: u32,
}

impl Buttons {
    /// No button pressed, the power button being bit `power`.
    #[must_use]
    pub const fn new(power: u8) -> Self {
        Buttons {
            power: 1 << (power % 8),
            sampled: 0,
            sampled_at: 0,
            state: 0,
            press_start: 0,
            press_finish: 0,
            power_start: 0,
            power_finish: 0,
        }
    }

    /// Takes the state sampled at `now`; what changed, if it was committed.
    pub fn update(&mut self, now: u32, sampled: u8) -> Change {
        if sampled != self.sampled {
            self.sampled_at = now;
            self.sampled = sampled;
        }
        if now.wrapping_sub(self.sampled_at) > DEBOUNCE_MS && sampled != self.state {
            return self.commit(now, sampled);
        }
        Change::default()
    }

    fn commit(&mut self, now: u32, state: u8) -> Change {
        let change = Change {
            pressed: state & !self.state,
            released: self.state & !state,
        };
        if change.pressed != 0 && self.state == 0 {
            self.press_start = now;
        }
        if change.released != 0 && state == 0 {
            self.press_finish = now;
        }
        if change.pressed & self.power != 0 {
            self.power_start = now;
        }
        if change.released & self.power != 0 {
            self.power_finish = now;
        }
        self.state = state;
        self.sampled = state;
        change
    }

    /// The committed state.
    #[must_use]
    pub fn state(&self) -> u8 {
        self.state
    }

    /// How long the buttons have been held at `now`, from the first one down; or how long the
    /// last press lasted, when none is down.
    #[must_use]
    pub fn held_ms(&self, now: u32) -> u32 {
        if self.state != 0 {
            now.wrapping_sub(self.press_start)
        } else {
            self.press_finish.wrapping_sub(self.press_start)
        }
    }

    /// The same for the power button alone.
    #[must_use]
    pub fn power_held_ms(&self, now: u32) -> u32 {
        if self.state & self.power != 0 {
            now.wrapping_sub(self.power_start)
        } else {
            self.power_finish.wrapping_sub(self.power_start)
        }
    }
}
