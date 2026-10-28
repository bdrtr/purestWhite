//! An ESP32-S3 app image (ESP-IDF's `esp_app_format.h`), checked as it streams in, before it is
//! written to an OTA app slot or while it is: its header, the chip it is for, its segments and
//! their checksum, its app descriptor, its size against the slot's, and where the SHA-256 that
//! ESP-IDF appends to it sits. Nothing here hashes: the caller hashes the bytes before
//! [`Checked::hash_at`] (on the device, with the chip's SHA unit) and compares the digest with the
//! 32 bytes there, and, to recognise a known image, with the digest it expects.
//!
//! The layout: a 24-byte header (magic `0xE9`, the segment count, the chip id at 12, the
//! "hash appended" flag at 23), then each segment as an 8-byte header (load address, length) and
//! its data, then padding so that the checksum byte (`0xEF` and every data byte combined by
//! exclusive or) ends a 16-byte block, then the 32-byte digest.

#[cfg(test)]
mod tests;

/// The first byte of an app image.
pub const MAGIC: u8 = 0xE9;
/// The ESP32-S3's chip id in the header.
pub const CHIP_ESP32S3: u16 = 9;
/// The length of the appended SHA-256.
pub const DIGEST: usize = 32;

const HEADER: usize = 24;
const SEGMENT_HEADER: usize = 8;
const MAX_SEGMENTS: u8 = 16;
const CHECKSUM_SEED: u8 = 0xEF;
/// The app descriptor's start (`esp_app_desc_t`, at the start of the first segment's data): the
/// magic word, the secure version, two reserved words, the version, the project name.
const DESC: usize = 80;
const DESC_MAGIC: u32 = 0xABCD_5432;

/// Why an image is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// Not an app image.
    Magic,
    /// For another chip than the ESP32-S3: its id.
    Chip(u16),
    /// No segment, or more than ESP-IDF allows.
    Segments(u8),
    /// Larger than the slot.
    TooLarge,
    /// The segments' checksum does not hold.
    Checksum,
    /// No SHA-256 appended.
    NoDigest,
    /// No app descriptor at the start of the first segment.
    Descriptor,
    /// It ends early.
    Truncated,
    /// Bytes follow the appended digest.
    Trailing,
}

/// What the app descriptor names (each up to 32 bytes, NUL padded).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppDesc {
    pub version: [u8; 32],
    pub project: [u8; 32],
}

/// A whole image that passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checked {
    /// Its length in bytes, the digest included.
    pub len: usize,
    /// Where the appended SHA-256 starts: it is the digest of the bytes before it.
    pub hash_at: usize,
    pub desc: AppDesc,
}

/// Where the check is in the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Header,
    SegmentHeader,
    SegmentData,
    Padding,
    Checksum,
    Digest,
    Done,
}

/// The check of an image fed in pieces of any size (`feed`), then ended (`finish`).
#[derive(Clone, Debug)]
pub struct ImageCheck {
    /// The slot's size.
    limit: usize,
    pos: usize,
    part: Part,
    /// The header or the segment header being read, and how much of it is.
    head: [u8; HEADER],
    have: usize,
    segments: u8,
    segment: u8,
    /// What is left of the segment's data, or of the digest.
    left: usize,
    xor: u8,
    desc: [u8; DESC],
    desc_have: usize,
}

impl ImageCheck {
    /// A check for a slot of `limit` bytes.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        ImageCheck {
            limit,
            pos: 0,
            part: Part::Header,
            head: [0; HEADER],
            have: 0,
            segments: 0,
            segment: 0,
            left: 0,
            xor: CHECKSUM_SEED,
            desc: [0; DESC],
            desc_have: 0,
        }
    }

    /// Takes the next bytes of the image.
    ///
    /// # Errors
    /// The first thing wrong in the image so far; the check stops there.
    pub fn feed(&mut self, mut data: &[u8]) -> Result<(), ImageError> {
        while !data.is_empty() {
            let used = self.step(data)?;
            self.pos += used;
            if self.pos > self.limit {
                return Err(ImageError::TooLarge);
            }
            data = data.get(used..).unwrap_or_default();
        }
        Ok(())
    }

    /// Ends the check: the image, once it has all come.
    ///
    /// # Errors
    /// It ended early, or its app descriptor is missing.
    pub fn finish(self) -> Result<Checked, ImageError> {
        if self.part != Part::Done {
            return Err(ImageError::Truncated);
        }
        let magic = self.desc.first_chunk::<4>().map(|w| u32::from_le_bytes(*w));
        let (Some(DESC_MAGIC), DESC) = (magic, self.desc_have) else {
            return Err(ImageError::Descriptor);
        };
        let field = |at: usize| -> [u8; 32] {
            let mut out = [0; 32];
            if let Some(bytes) = self.desc.get(at..at + 32) {
                out.copy_from_slice(bytes);
            }
            out
        };
        Ok(Checked {
            len: self.pos,
            hash_at: self.pos - DIGEST,
            desc: AppDesc {
                version: field(16),
                project: field(48),
            },
        })
    }

    /// Takes what it can of `data` for the part it is in: how many bytes.
    fn step(&mut self, data: &[u8]) -> Result<usize, ImageError> {
        match self.part {
            Part::Header => {
                let used = self.fill(HEADER, data);
                if self.have == HEADER {
                    self.header()?;
                }
                Ok(used)
            }
            Part::SegmentHeader => {
                let used = self.fill(SEGMENT_HEADER, data);
                if self.have == SEGMENT_HEADER {
                    self.segment_header()?;
                }
                Ok(used)
            }
            Part::SegmentData => Ok(self.segment_data(data)),
            Part::Padding => {
                // The checksum byte ends a 16-byte block: the bytes up to it are skipped (none
                // when it is next, and the checksum then takes the byte).
                let need = 15 - self.pos % 16;
                let used = need.min(data.len());
                if used == need {
                    self.part = Part::Checksum;
                }
                Ok(used)
            }
            Part::Checksum => {
                if data.first() != Some(&self.xor) {
                    return Err(ImageError::Checksum);
                }
                self.part = Part::Digest;
                self.left = DIGEST;
                Ok(1)
            }
            Part::Digest => {
                let used = self.left.min(data.len());
                self.left -= used;
                if self.left == 0 {
                    self.part = Part::Done;
                }
                Ok(used)
            }
            Part::Done => Err(ImageError::Trailing),
        }
    }

    /// Copies into `head` what `data` has of the `len` bytes being read: how many.
    fn fill(&mut self, len: usize, data: &[u8]) -> usize {
        let used = (len - self.have).min(data.len());
        if let (Some(to), Some(from)) = (
            self.head.get_mut(self.have..self.have + used),
            data.get(..used),
        ) {
            to.copy_from_slice(from);
        }
        self.have += used;
        used
    }

    fn header(&mut self) -> Result<(), ImageError> {
        let [
            magic,
            segments,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            chip_low,
            chip_high,
            ..,
            digest,
        ] = self.head;
        if magic != MAGIC {
            return Err(ImageError::Magic);
        }
        let chip = u16::from_le_bytes([chip_low, chip_high]);
        if chip != CHIP_ESP32S3 {
            return Err(ImageError::Chip(chip));
        }
        if segments == 0 || segments > MAX_SEGMENTS {
            return Err(ImageError::Segments(segments));
        }
        if digest != 1 {
            return Err(ImageError::NoDigest);
        }
        self.segments = segments;
        self.have = 0;
        self.part = Part::SegmentHeader;
        Ok(())
    }

    fn segment_header(&mut self) -> Result<(), ImageError> {
        let [_, _, _, _, l0, l1, l2, l3, ..] = self.head;
        let len = u32::from_le_bytes([l0, l1, l2, l3]);
        let len = usize::try_from(len).map_err(|_| ImageError::TooLarge)?;
        if len > self.limit {
            return Err(ImageError::TooLarge);
        }
        self.left = len;
        self.have = 0;
        self.part = Part::SegmentData;
        if len == 0 {
            self.next_segment();
        }
        Ok(())
    }

    fn segment_data(&mut self, data: &[u8]) -> usize {
        let used = self.left.min(data.len());
        let bytes = data.get(..used).unwrap_or_default();
        self.xor = bytes.iter().fold(self.xor, |x, b| x ^ b);
        if self.segment == 0 {
            let room = DESC - self.desc_have;
            let take = room.min(bytes.len());
            if let (Some(to), Some(from)) = (
                self.desc.get_mut(self.desc_have..self.desc_have + take),
                bytes.get(..take),
            ) {
                to.copy_from_slice(from);
            }
            self.desc_have += take;
        }
        self.left -= used;
        if self.left == 0 {
            self.next_segment();
        }
        used
    }

    fn next_segment(&mut self) {
        self.segment += 1;
        self.part = if self.segment == self.segments {
            Part::Padding
        } else {
            Part::SegmentHeader
        };
    }
}
