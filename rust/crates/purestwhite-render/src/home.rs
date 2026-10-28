//! The home screen: the time at the top left and the battery at the top right, the name in the
//! middle with a hint under it.

use crate::frame::{HEIGHT, WIDTH};
use crate::text::Text;
use bembeyaz_gfx::{Canvas, Fill};
use core::fmt::Write as _;
use purestwhite_app::view::{Home, Time};

/// The top bar's height and margin.
const BAR: i32 = 48;
const MARGIN: i32 = 16;
/// The battery's outline, its terminal, and the gap between the outline and the fill.
const BATTERY_WIDTH: i32 = 48;
const BATTERY_HEIGHT: i32 = 22;
const TERMINAL: i32 = 4;
const INSET: i32 = 3;

pub fn draw(c: &mut Canvas<'_>, home: Home, text: Option<&mut Text<'_>>) {
    c.fill_rect(0, 0, WIDTH, HEIGHT, Fill::White);
    if let Some(percent) = home.battery {
        draw_battery(c, percent);
    }
    c.fill_rect(MARGIN, BAR, WIDTH - 2 * MARGIN, 2, Fill::Black);
    let Some(text) = text else {
        return;
    };
    if let Some(time) = home.time {
        let y = (BAR - text.line_height()) / 2;
        text.draw(c, MARGIN, y, clock(time).as_str());
    }
    let middle = HEIGHT / 2 - text.line_height();
    text.draw_centered(c, middle, "purestWhite");
    text.draw_centered(c, middle + 2 * text.line_height(), "Hold Power to sleep");
}

/// A battery at the top right, filled in proportion to `percent`.
fn draw_battery(c: &mut Canvas<'_>, percent: u8) {
    let x = WIDTH - MARGIN - BATTERY_WIDTH - TERMINAL;
    let y = (BAR - BATTERY_HEIGHT) / 2;
    c.rect(x, y, BATTERY_WIDTH, BATTERY_HEIGHT, 2, true);
    c.fill_rect(
        x + BATTERY_WIDTH,
        y + BATTERY_HEIGHT / 3,
        TERMINAL,
        BATTERY_HEIGHT / 3,
        Fill::Black,
    );
    let room = BATTERY_WIDTH - 2 * INSET;
    let filled = room * i32::from(percent.min(100)) / 100;
    c.fill_rect(
        x + INSET,
        y + INSET,
        filled,
        BATTERY_HEIGHT - 2 * INSET,
        Fill::Black,
    );
}

/// "HH:MM".
fn clock(time: Time) -> Clock {
    let mut clock = Clock::default();
    let _ = write!(clock, "{:02}:{:02}", time.hour % 24, time.minute % 60);
    clock
}

/// A short text without allocating: enough for "HH:MM".
#[derive(Default)]
struct Clock {
    bytes: [u8; 5],
    len: usize,
}

impl Clock {
    fn as_str(&self) -> &str {
        core::str::from_utf8(self.bytes.get(..self.len).unwrap_or_default()).unwrap_or_default()
    }
}

impl core::fmt::Write for Clock {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.len + s.len();
        let room = self.bytes.get_mut(self.len..end).ok_or(core::fmt::Error)?;
        room.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}
