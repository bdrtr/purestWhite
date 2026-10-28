//! The planes: a frame's rows sent to DTM1 or DTM2, bottom row first, padded white to the
//! addressed gates.

use super::{MAX_ROW, Uc8179};
use crate::{BusError, Interface};

/// A plane's rows, before the white padding.
#[derive(Clone, Copy)]
pub(super) enum Rows<'f> {
    White,
    Frame(&'f [u8]),
    Inverted(&'f [u8]),
    /// The two frames XOR-ed.
    Xor(&'f [u8], &'f [u8]),
}

/// The `wb` bytes from `start` of `f`, when the row is visible (`start` set).
fn row_of(f: &[u8], start: Option<usize>, wb: usize) -> Option<&[u8]> {
    start.and_then(|s| f.get(s..s + wb))
}

impl<B: AsRef<[u8]> + AsMut<[u8]>> Uc8179<B> {
    /// A plane: `rows` bottom first, then white rows to the addressed height.
    pub(super) async fn plane(
        &self,
        bus: &mut impl Interface,
        command: u8,
        rows: Rows<'_>,
    ) -> Result<(), BusError> {
        let c = self.config;
        let wb = self.row_bytes().min(MAX_ROW);
        let mut row = [0u8; MAX_ROW];
        let row = row.get_mut(..wb).unwrap_or_default();
        bus.burst(command, row, c.tres_height, |n, row| {
            let start = (n < c.height).then(|| usize::from(c.height - 1 - n) * wb);
            let at = |f| row_of(f, start, wb);
            let written = match rows {
                Rows::White => None,
                Rows::Frame(f) => at(f).map(|src| row.copy_from_slice(src)),
                Rows::Inverted(f) => at(f).map(|src| {
                    for (to, &from) in row.iter_mut().zip(src) {
                        *to = !from;
                    }
                }),
                Rows::Xor(a, b) => at(a).zip(at(b)).map(|(a, b)| {
                    for ((to, &x), &y) in row.iter_mut().zip(a).zip(b) {
                        *to = x ^ y;
                    }
                }),
            };
            if written.is_none() {
                row.fill(0xFF);
            }
        })
        .await
    }
}
