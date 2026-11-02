//! The gauge's readings during a battery measurement and the report's line for them.

use core::fmt::{self, Display, Formatter};

/// How many readings a measurement keeps: one at its begin, one every five minutes after, and one
/// at its end; an hour's loop fills them.
pub const LOG: usize = 13;

/// A reading of the gauge: either part may be missing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// The state of charge, 0 to 100.
    pub percent: Option<u8>,
    /// The cell's voltage (the CW2017 gives at most 5120 mV).
    pub millivolts: Option<u16>,
}

/// A measurement's readings, the last place kept for the one at its end, and how many pages the
/// loop showed once it ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Readings {
    noted: [Reading; LOG],
    count: usize,
    pages: Option<u16>,
}

impl Readings {
    #[must_use]
    pub const fn new() -> Self {
        Readings {
            noted: [Reading {
                percent: None,
                millivolts: None,
            }; LOG],
            count: 0,
            pages: None,
        }
    }

    /// How many readings were noted, failed ones included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// How many pages the loop showed, once the measurement ended.
    #[must_use]
    pub fn pages(&self) -> Option<u16> {
        self.pages
    }

    /// A reading on the beat, while there is room for it beside the end's.
    pub(crate) fn note_beat(&mut self, reading: Reading) {
        if self.count + 1 < LOG {
            self.push(reading);
        }
    }

    /// The reading at the measurement's end, after `pages` pages.
    pub(crate) fn note_end(&mut self, reading: Reading, pages: u16) {
        self.push(reading);
        self.pages = Some(pages);
    }

    fn push(&mut self, reading: Reading) {
        if let Some(entry) = self.noted.get_mut(self.count) {
            *entry = reading;
            self.count += 1;
        }
    }

    fn noted(&self) -> &[Reading] {
        self.noted.get(..self.count).unwrap_or_default()
    }
}

/// `battery: 87% 4012 3998 - 3990 86%, 120 pages`: each reading's millivolts (`-` when the gauge
/// gave none) between the first and the last charge given, and the pages once the measurement
/// ended; at most 94 bytes with the CW2017's 4-digit millivolts and up to 999 pages.
impl Display for Readings {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        out.write_str("battery:")?;
        let mut charges = self.noted().iter().filter_map(|r| r.percent);
        let (first, last) = (charges.next(), charges.next_back());
        if let Some(first) = first {
            write!(out, " {first}%")?;
        }
        for reading in self.noted() {
            match reading.millivolts {
                Some(mv) => write!(out, " {mv}")?,
                None => out.write_str(" -")?,
            }
        }
        if let Some(last) = last.or(first) {
            write!(out, " {last}%")?;
        }
        if let Some(pages) = self.pages {
            write!(out, ", {pages} pages")?;
        }
        Ok(())
    }
}
