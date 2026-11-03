//! A scan's tally: each advertiser once (by address), whether it is a HID remote, and the
//! strongest remote's signal. When it is full, a remote takes the place of an advertiser that is
//! not one, as the C++ firmware's SDK keeps the remotes in a crowded scan. No names or addresses
//! leave it: the report goes on the card.

use crate::Advert;
use core::fmt::{self, Display, Formatter};

/// How many advertisers a scan tells apart; more are counted in `more`.
pub const SEEN_MOST: usize = 64;

/// A report's RSSI that means "not available" (HCI).
const NO_RSSI: i8 = 127;

/// One advertiser: its address (kind and 6 bytes), what it said, and its strongest signal.
#[derive(Clone, Copy)]
struct Entry {
    addr: (u8, [u8; 6]),
    advert: Advert,
    rssi: Option<i8>,
}

/// The advertisers a scan heard.
pub struct Seen {
    entries: [Option<Entry>; SEEN_MOST],
    count: usize,
    /// Reports from advertisers past `SEEN_MOST` (each may count more than once); a remote among
    /// them took the place of one that is not.
    more: usize,
}

impl Seen {
    /// An empty tally.
    #[must_use]
    pub const fn new() -> Self {
        Seen {
            entries: [None; SEEN_MOST],
            count: 0,
            more: 0,
        }
    }

    /// A report from the advertiser at `addr` (its address kind and bytes): `data` read and taken
    /// with what it said before, its strongest signal kept.
    pub fn report(&mut self, addr: (u8, [u8; 6]), data: &[u8], rssi: i8) {
        let advert = Advert::read(data);
        let rssi = (rssi != NO_RSSI).then_some(rssi);
        if let Some(entry) = self.entries.iter_mut().flatten().find(|e| e.addr == addr) {
            entry.advert = entry.advert.merged(advert);
            entry.rssi = entry.rssi.max(rssi);
            return;
        }
        let entry = Entry { addr, advert, rssi };
        if let Some(slot) = self.entries.get_mut(self.count) {
            *slot = Some(entry);
            self.count += 1;
            return;
        }
        self.more += 1;
        if advert.is_hid() {
            let other = self
                .entries
                .iter_mut()
                .find(|e| e.is_some_and(|e| !e.advert.is_hid()));
            if let Some(slot) = other {
                *slot = Some(entry);
            }
        }
    }

    /// How many advertisers were told apart.
    #[must_use]
    pub fn devices(&self) -> usize {
        self.count
    }

    /// How many of them are HID remotes.
    #[must_use]
    pub fn remotes(&self) -> usize {
        self.entries().filter(|e| e.advert.is_hid()).count()
    }

    /// The strongest remote's signal, in dBm.
    #[must_use]
    pub fn best_remote(&self) -> Option<i8> {
        self.entries()
            .filter(|e| e.advert.is_hid())
            .filter_map(|e| e.rssi)
            .max()
    }

    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().flatten()
    }
}

impl Default for Seen {
    fn default() -> Self {
        Self::new()
    }
}

/// `ble: 12 devices, 1 hid, best hid -48 dBm` (`64+ devices` past `SEEN_MOST`), or
/// `ble: 12 devices, no hid`.
impl Display for Seen {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let more = if self.more > 0 { "+" } else { "" };
        write!(out, "ble: {}{more} devices, ", self.count)?;
        match self.best_remote() {
            Some(dbm) => write!(out, "{} hid, best hid {dbm} dBm", self.remotes()),
            None => out.write_str("no hid"),
        }
    }
}
