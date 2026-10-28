//! The report an image leaves the C++ firmware in RTC slow memory.

/// Where the report goes: the last 256 bytes of RTC slow memory, which the C++ firmware leaves
/// unused (it checks that at boot). lib/hal/HalSystem.cpp reads it with the same layout: magic,
/// length, then the text. A change of layout changes the magic.
const REPORT_ADDR: usize = 0x5000_1F00;
const REPORT_MAGIC: u32 = 0x5057_4254; // "PWBT"
const REPORT_TEXT: usize = 248;

/// The report's text, saved again after every stage so a run that dies halfway still says how far
/// it got. Text past its room is dropped and the last byte becomes `~`, so a cut report does not
/// read as a run that stopped.
pub struct Report {
    text: [u8; REPORT_TEXT],
    len: usize,
}

impl Default for Report {
    fn default() -> Self {
        Report {
            text: [0; REPORT_TEXT],
            len: 0,
        }
    }
}

impl core::fmt::Write for Report {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if let Some(slot) = self.text.get_mut(self.len) {
                *slot = b;
                self.len += 1;
            } else if let Some(last) = self.text.last_mut() {
                *last = b'~';
            }
        }
        Ok(())
    }
}

impl Report {
    /// The bytes written so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Drops what was written after the first `len` bytes, to write it again.
    pub fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.text.get(..self.len).unwrap_or_default()).unwrap_or("")
    }

    /// Leaves the report for the C++ firmware's next boot.
    pub fn save(&self) {
        let base = REPORT_ADDR as *mut u32;
        // SAFETY: RTC slow memory is plain RAM that nothing else in these images uses at this
        // address; the C++ firmware reads it only after the next reset.
        #[allow(clippy::cast_possible_truncation)]
        unsafe {
            base.add(1).write_volatile(self.len as u32);
            let text = base.add(2).cast::<u8>();
            for (i, &b) in self.text.iter().enumerate() {
                text.add(i).write_volatile(b);
            }
            base.write_volatile(REPORT_MAGIC);
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    /// Adds a line to the report saved last, for a panic, which cannot reach the `Report`.
    pub fn append_saved(line: &str) {
        let base = REPORT_ADDR as *mut u32;
        // SAFETY: as `save`; the length is checked against the room before any write.
        unsafe {
            if base.read_volatile() != REPORT_MAGIC {
                return;
            }
            let mut len = base.add(1).read_volatile() as usize;
            let text = base.add(2).cast::<u8>();
            for &b in line.as_bytes() {
                if len >= REPORT_TEXT {
                    break;
                }
                text.add(len).write_volatile(b);
                len += 1;
            }
            #[allow(clippy::cast_possible_truncation)]
            base.add(1).write_volatile(len as u32);
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
}
