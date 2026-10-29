//! The boot counter (rust/firmware/DESIGN.md, the update gate): how many runs in a row ended
//! before they reached a healthy state, kept in RTC memory across resets. Each run marks itself
//! running at its start ([`Runs::begin`]) and clears the mark once it is healthy
//! ([`Runs::healthy`]); a run that ends otherwise (a panic, a reset, a hang the watchdog ends, a
//! brownout that keeps RTC memory) leaves the mark, and the next run counts it. At [`LIMIT`] in a
//! row the firmware is due to go to its recovery mode. After a power-on, whatever RTC memory holds
//! is ignored.
//!
//! It counts unfinished runs rather than reset reasons: during phase 2 a run that crashes is
//! followed by the C++ firmware, so the reset reason the next bare-metal run sees is the C++
//! firmware's restart, not the crash.
//!
//! Nothing here touches memory: the firmware reads the record's words, and writes back the ones
//! [`Runs::encode`] gives.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests;

/// Unfinished runs in a row at which the firmware is due to go to its recovery mode.
pub const LIMIT: u8 = 3;
/// The record's length in 32-bit words.
pub const WORDS: usize = 4;
/// The record's first word ("PWBC"); a change of layout changes it.
const MAGIC: u32 = 0x5057_4243;
/// In the record's value: the unfinished runs in the low byte, then the running mark.
const RUNNING: u32 = 1 << 8;

/// What the record says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Runs {
    /// Runs in a row that ended before they were healthy.
    pub unfinished: u8,
    /// The run that wrote it has not reached a healthy state (yet).
    pub running: bool,
}

impl Runs {
    /// The record as it is kept: the magic, the value, its inverted copy, and a spare zero word.
    #[must_use]
    pub fn encode(self) -> [u32; WORDS] {
        let value = u32::from(self.unfinished) | if self.running { RUNNING } else { 0 };
        [MAGIC, value, !value, 0]
    }

    /// The record in `words`, when its magic and inverted copy hold and its value is one
    /// [`Runs::encode`] gives; `None` otherwise (memory never written, or written by something
    /// else).
    #[must_use]
    pub fn decode(words: [u32; WORDS]) -> Option<Runs> {
        let [magic, value, inverted, _] = words;
        if magic != MAGIC || inverted != !value || value & !(RUNNING | 0xFF) != 0 {
            return None;
        }
        Some(Runs {
            unfinished: u8::try_from(value & 0xFF).ok()?,
            running: value & RUNNING != 0,
        })
    }

    /// This run's record at its start, from the record the last run left (`saved`, `None` when
    /// there is none) and whether the chip was just powered on: the last run counted when it
    /// was still running, and this one marked running.
    #[must_use]
    pub fn begin(saved: Option<Runs>, powered_on: bool) -> Runs {
        let last = if powered_on { None } else { saved };
        let unfinished = last.map_or(0, |last| {
            last.unfinished.saturating_add(u8::from(last.running))
        });
        Runs {
            unfinished,
            running: true,
        }
    }

    /// The record once the run is healthy: nothing unfinished, not running.
    #[must_use]
    pub fn healthy() -> Runs {
        Runs::default()
    }

    /// Whether the firmware is due to go to its recovery mode: [`LIMIT`] or more unfinished runs
    /// in a row.
    #[must_use]
    pub fn recovery_due(self) -> bool {
        self.unfinished >= LIMIT
    }
}

/// The record for a report: `runs: N unfinished`, then `, recovery due` when it is.
impl core::fmt::Display for Runs {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(out, "runs: {} unfinished", self.unfinished)?;
        if self.recovery_due() {
            write!(out, ", recovery due")?;
        }
        Ok(())
    }
}
