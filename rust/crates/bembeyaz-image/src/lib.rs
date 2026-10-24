//! What image files are, from their first bytes: the chapter parser's image size probe, fed a
//! JPEG or PNG as it streams out of the EPUB and done as soon as the size is known. JPEGs are
//! walked marker by marker (segments of any size skipped without buffering) up to a frame header;
//! a PNG's size is at fixed offsets of its IHDR chunk.
//!
//! Nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Where a probe is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    /// The first byte tells the format.
    #[default]
    Sniff,
    /// A PNG's signature and IHDR, at fixed offsets.
    PngHeader,
    /// SOI's second byte.
    JpegSoi,
    /// A marker's 0xFF.
    JpegFf,
    /// A marker's type (or fill bytes).
    JpegMarker,
    /// A segment length's high byte, then its low byte.
    JpegLengthHigh,
    JpegLengthLow,
    /// The body of a segment that is not a frame header.
    JpegSkip,
    /// A frame header's precision, height and width.
    JpegFrame,
    Done,
    Failed,
}

/// A size probe part way through a file; the default is a new one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    state: State,
    /// Bytes of a PNG read so far.
    pos: u32,
    /// Bytes of the JPEG segment still to skip.
    skip_left: u32,
    width: u32,
    height: u32,
    segment_len: u16,
    /// The segment being read is a frame header.
    frame_pending: bool,
    /// Bytes of the frame header collected.
    frame_fill: u8,
    frame: [u8; 5],
}

/// Whether JPEG marker `marker` starts a frame header (`SOFn`), which holds the size: the 0xCn
/// markers but DHT (C4), JPG (C8) and DAC (CC).
fn is_frame(marker: u8) -> bool {
    (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC)
}

impl Probe {
    /// Reads `data` until the size is known or the file is known not to have one, and returns how
    /// many bytes it took before the one that settled it (all of them if neither happened).
    pub fn feed(&mut self, data: &[u8]) -> usize {
        for (i, &b) in data.iter().enumerate() {
            if !self.byte(b) {
                return i;
            }
        }
        data.len()
    }

    /// The width and height, once found.
    #[must_use]
    pub fn size(&self) -> Option<(u32, u32)> {
        (self.state == State::Done).then_some((self.width, self.height))
    }

    /// Fails the probe.
    fn fail(&mut self) -> bool {
        self.state = State::Failed;
        false
    }

    /// Reads one byte; false once the probe is settled.
    fn byte(&mut self, b: u8) -> bool {
        match self.state {
            State::Sniff => {
                self.state = match b {
                    0xFF => State::JpegSoi,
                    0x89 => State::PngHeader,
                    _ => return self.fail(),
                };
                self.pos = 1;
                true
            }
            State::PngHeader => self.png(b),
            State::JpegSoi => {
                if b != 0xD8 {
                    return self.fail();
                }
                self.state = State::JpegFf;
                true
            }
            State::JpegFf => {
                if b != 0xFF {
                    return self.fail();
                }
                self.state = State::JpegMarker;
                true
            }
            State::JpegMarker => match b {
                // Fill bytes before a marker.
                0xFF => true,
                // TEM, RSTn and SOI stand alone, without a length.
                0x01 | 0xD0..=0xD8 => {
                    self.state = State::JpegFf;
                    true
                }
                // EOI or SOS before any frame header: no size to find.
                0xD9 | 0xDA => self.fail(),
                _ => {
                    self.frame_pending = is_frame(b);
                    self.state = State::JpegLengthHigh;
                    true
                }
            },
            State::JpegLengthHigh => {
                self.segment_len = u16::from(b) << 8;
                self.state = State::JpegLengthLow;
                true
            }
            State::JpegLengthLow => self.length_low(b),
            State::JpegSkip => {
                self.skip_left = self.skip_left.wrapping_sub(1);
                if self.skip_left == 0 {
                    self.state = State::JpegFf;
                }
                true
            }
            State::JpegFrame => {
                if let Some(slot) = self.frame.get_mut(usize::from(self.frame_fill)) {
                    *slot = b;
                }
                self.frame_fill = self.frame_fill.wrapping_add(1);
                if self.frame_fill >= 5 {
                    let [_, h0, h1, w0, w1] = self.frame;
                    self.height = u32::from(u16::from_be_bytes([h0, h1]));
                    self.width = u32::from(u16::from_be_bytes([w0, w1]));
                    self.state = State::Done;
                    return false;
                }
                true
            }
            State::Done | State::Failed => false,
        }
    }
    /// Reads byte `pos` of a PNG: bytes 1 to 7 are the signature, 8 to 11 IHDR's length, 12 to 15
    /// "IHDR" and 16 to 23 the width and height.
    fn png(&mut self, b: u8) -> bool {
        let pos = self.pos;
        match pos {
            1..8 => {
                if PNG_SIGNATURE.get(pos as usize) != Some(&b) {
                    return self.fail();
                }
            }
            12..16 => {
                if b"IHDR".get(pos as usize - 12) != Some(&b) {
                    return self.fail();
                }
            }
            16..20 => self.width = self.width << 8 | u32::from(b),
            20..24 => {
                self.height = self.height << 8 | u32::from(b);
                if pos == 23 {
                    self.state = State::Done;
                    self.pos = pos + 1;
                    return false;
                }
            }
            _ => {}
        }
        self.pos = pos.wrapping_add(1);
        true
    }

    /// Reads the low byte of a JPEG segment's length, which counts itself.
    fn length_low(&mut self, b: u8) -> bool {
        let len = self.segment_len | u16::from(b);
        self.segment_len = len;
        let frame = self.frame_pending;
        if len < 2 || (frame && len < 7) {
            return self.fail();
        }
        if frame {
            self.frame_fill = 0;
            self.state = State::JpegFrame;
        } else if len == 2 {
            self.state = State::JpegFf;
        } else {
            self.skip_left = u32::from(len) - 2;
            self.state = State::JpegSkip;
        }
        true
    }
}

#[cfg(test)]
mod tests;
