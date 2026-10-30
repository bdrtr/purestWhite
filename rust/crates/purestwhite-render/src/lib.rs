//! Draws the application's views (`purestwhite_app::view`) into a frame of the X4 Pro's panel:
//! pure, so the device, the host simulator and the tests draw the same pixels. Text needs a font
//! ([`Text`]); without one, only the shapes are drawn.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

mod frame;
mod home;
mod sleep;
#[cfg(test)]
mod tests;
pub mod text;

pub use frame::{FRAME_BYTES, HEIGHT, WIDTH, canvas, is_black, set_black};
use purestwhite_app::view::View;
pub use text::{FaceError, Text, Writer};

/// Draws `view` over the whole of `frame` (`FRAME_BYTES` long, a bit per pixel, 1 white), with
/// its text in `text` when given.
pub fn draw(view: &View, frame: &mut [u8], text: Option<&mut Text<'_>>) {
    let mut canvas = canvas(frame);
    match view {
        View::Home(home) => home::draw(&mut canvas, *home, text),
        View::Sleep => sleep::draw(&mut canvas, text),
    }
}
