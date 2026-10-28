//! What the run did: the presses and gestures drawn, and the refreshes, for the report.

use crate::draw::{KEYS, draw_long_press, draw_swipe, draw_tap, draw_tick};
use alloc::format;
use alloc::string::String;
use bembeyaz_gfx::Canvas;
use core::fmt::Write as _;
use purestwhite_epd::uc8279x4::Refresh;
use purestwhite_firmware::input::Polled;
use purestwhite_firmware::test_image::Report;
use purestwhite_touch::Point;

/// After this many fast refreshes, a half refresh clears what they left behind.
const FAST_RUN: u32 = 15;

#[derive(Default)]
pub struct Counts {
    /// Presses of Left, Right, Power and taps of Home.
    keys: [u32; KEYS],
    taps: u32,
    swipes: u32,
    long_presses: u32,
    fast: u32,
    half: u32,
    failed: u32,
    /// The slowest refresh, in milliseconds.
    slowest_ms: u64,
    /// Fast refreshes since the last half or full one.
    fast_run: u32,
}

impl Counts {
    /// Draws what `polled` gave; whether anything was drawn.
    pub fn draw(&mut self, polled: &Polled, c: &mut Canvas<'_>) -> bool {
        let mut drawn = false;
        for key in 0..3 {
            if polled.buttons.pressed & (1 << key) != 0 {
                drawn |= self.tick(key, c);
            }
        }
        let t = &polled.touch;
        if t.home_tapped {
            drawn |= self.tick(3, c);
        }
        // Touch points are panel pixels: the touch panel's range is the panel's 800 by 480. The
        // C++ firmware maps them through fractions (`tapToLogical`); without floats here, which
        // eink-qemu 0.9.2 fails on.
        let at = |c: &Canvas<'_>, p: Point| c.panel_to_logical(i32::from(p.x), i32::from(p.y));
        if let Some(p) = t.tap {
            draw_tap(c, at(c, p));
            self.taps += 1;
            drawn = true;
        }
        if let Some(p) = t.long_press {
            draw_long_press(c, at(c, p));
            self.long_presses += 1;
            drawn = true;
        }
        if let Some((start, end)) = t.swipe {
            draw_swipe(c, at(c, start), at(c, end));
            self.swipes += 1;
            drawn = true;
        }
        drawn
    }

    /// The next tick under `key`'s box; whether there is such a box.
    fn tick(&mut self, key: usize, c: &mut Canvas<'_>) -> bool {
        let Some(count) = self.keys.get_mut(key) else {
            return false;
        };
        draw_tick(c, key, *count);
        *count += 1;
        true
    }

    /// The counts of the gestures, for the line at the bottom of the screen.
    pub fn status(&self) -> String {
        format!(
            "dokun {}   kaydır {}   uzun {}",
            self.taps, self.swipes, self.long_presses
        )
    }

    /// The refresh to show the next frame with: `Full` when asked, otherwise `Fast`, or `Half`
    /// after `FAST_RUN` fast ones.
    pub fn next_refresh(&mut self, full: bool) -> Refresh {
        if full {
            self.fast_run = 0;
            Refresh::Full
        } else if self.fast_run >= FAST_RUN {
            self.fast_run = 0;
            self.half += 1;
            Refresh::Half
        } else {
            self.fast_run += 1;
            Refresh::Fast
        }
    }

    /// A refresh done in `ms`, which worked or not.
    pub fn note_refresh(&mut self, refresh: Refresh, ms: u64, worked: bool) {
        self.slowest_ms = self.slowest_ms.max(ms);
        if refresh == Refresh::Fast {
            self.fast += 1;
        }
        if !worked {
            self.failed += 1;
        }
    }

    pub fn write(&self, report: &mut Report) {
        // Short, so that the sleep line's marks keep their room in the report's 248 bytes.
        let _ = writeln!(
            report,
            "hizli {} yarim {} hata {} {} ms",
            self.fast, self.half, self.failed, self.slowest_ms
        );
        let [left, right, power, home] = self.keys;
        let _ = writeln!(
            report,
            "tus {left}/{right}/{power}/{home} dokun {} kaydir {} uzun {}",
            self.taps, self.swipes, self.long_presses
        );
    }
}
