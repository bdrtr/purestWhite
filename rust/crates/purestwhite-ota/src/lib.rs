//! ESP-IDF's otadata with two OTA app slots, as its bootloader reads it: which slot the entry in
//! force selects, and the entry that selects the other slot. It picks the entry in force, the
//! next sequence number and the sector as the C++ firmware's `ota_boot::switchTo` does
//! (src/network/OtaBootSwitch.cpp), but writes the entry valid rather than new, so that the
//! bootloader does not roll back to the running image, and writes nothing when the other slot is
//! already selected and valid. Nothing here touches the flash: the caller reads the two entries,
//! one at the start of each of otadata's two sectors, and writes the one [`select_other`] gives.
//!
//! [`image`] checks an app image as it streams in, [`digest`] works out its SHA-256, and [`slot`]
//! writes it into a slot, sector by sector, read back: the SD updater's parts.
//!
//! The bootloader takes, of the two entries whose CRC holds and whose state is neither INVALID nor
//! ABORTED, the one with the higher sequence number; sequence number `seq` selects slot
//! `(seq - 1) % 2`.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

pub mod digest;
pub mod image;
pub mod slot;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

/// An entry's size in bytes: the sequence number at 0, a label, the state at 24 and the CRC at 28.
pub const ENTRY: usize = 32;
/// Where the second entry is, from the first: otadata's second sector.
pub const SECTOR: u32 = 0x1000;

/// An entry as it is on the flash.
pub type Entry = [u8; ENTRY];

/// An entry's state (ESP-IDF's `esp_ota_img_states_t`): new, then pending verify (the bootloader
/// rolls back an image that does not confirm itself), then valid.
pub const STATE_NEW: u32 = 0;
pub const STATE_PENDING_VERIFY: u32 = 1;
pub const STATE_VALID: u32 = 2;
const STATE_INVALID: u32 = 3;
const STATE_ABORTED: u32 = 4;
/// The sequence number of an erased entry, which the bootloader ignores.
const SEQ_ERASED: u32 = u32::MAX;

/// An OTA app slot: `ota_0` or `ota_1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Ota0,
    Ota1,
}

impl Slot {
    /// The slot of an app partition's OTA number (`ota_0` is 0); `None` past the two.
    #[must_use]
    pub fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Slot::Ota0),
            1 => Some(Slot::Ota1),
            _ => None,
        }
    }

    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Slot::Ota0 => Slot::Ota1,
            Slot::Ota1 => Slot::Ota0,
        }
    }

    /// The slot sequence number `seq` selects.
    #[must_use]
    pub fn of_seq(seq: u32) -> Self {
        if seq.wrapping_sub(1).is_multiple_of(2) {
            Slot::Ota0
        } else {
            Slot::Ota1
        }
    }
}

/// An entry the bootloader takes: its sequence number and state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InForce {
    seq: u32,
    state: u32,
}

/// An entry to write, and which sector it goes to (0 or 1): the one that is not in force, so
/// that a power loss while it is written leaves the entry in force as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Write {
    pub sector: u32,
    pub entry: Entry,
}

/// What [`select_other`] found and what it would write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    /// The state of the entry in force when it selects the running image's slot (see
    /// `STATE_PENDING_VERIFY`); `None` when it selects the other slot, or none is in force.
    pub booted_state: Option<u32>,
    /// The entry to write; `None` when the entry in force already selects the other slot, valid.
    pub write: Option<Write>,
}

/// The sequence numbers ran out: the next one would be the erased entry's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeqExhausted;

/// ESP-IDF's `esp_rom_crc32_le(UINT32_MAX, ...)` over a sequence number's four bytes, which an
/// entry's CRC is.
#[must_use]
pub fn seq_crc(seq: u32) -> u32 {
    let mut crc = 0u32;
    for b in seq.to_le_bytes() {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xEDB8_8320 } else { 0 };
        }
    }
    !crc
}

/// What selects the slot other than `booted` (the running image's) for the next boot, valid,
/// given the two entries (sector 0's, then sector 1's).
///
/// # Errors
/// The sequence numbers ran out.
pub fn select_other(entries: &[Entry; 2], booted: Slot) -> Result<Selection, SeqExhausted> {
    let target = booted.other();
    let [first, second] = entries;
    // The entry in force, and the sector a new one goes to: the other one.
    let (active, sector) = match (in_force(first), in_force(second)) {
        (Some(a), Some(b)) if b.seq > a.seq => (Some(b), 0),
        (Some(a), _) => (Some(a), 1),
        (None, Some(b)) => (Some(b), 0),
        (None, None) => (None, 0),
    };
    let booted_state = active
        .filter(|a| Slot::of_seq(a.seq) == booted)
        .map(|a| a.state);
    if let Some(a) = active
        && Slot::of_seq(a.seq) == target
        && a.state == STATE_VALID
    {
        return Ok(Selection {
            booted_state,
            write: None,
        });
    }
    let seq = next_seq(active, target).ok_or(SeqExhausted)?;
    let write = Write {
        sector,
        entry: valid_entry(seq),
    };
    Ok(Selection {
        booted_state,
        write: Some(write),
    })
}

/// The little-endian word at `at`.
fn word(entry: &Entry, at: usize) -> u32 {
    let mut bytes = [0; 4];
    bytes.copy_from_slice(entry.get(at..at + 4).unwrap_or(&[0; 4]));
    u32::from_le_bytes(bytes)
}

/// The entry, when the bootloader would take it.
fn in_force(entry: &Entry) -> Option<InForce> {
    let (seq, state, crc) = (word(entry, 0), word(entry, 24), word(entry, 28));
    (seq != SEQ_ERASED && crc == seq_crc(seq) && state != STATE_INVALID && state != STATE_ABORTED)
        .then_some(InForce { seq, state })
}

/// The next sequence number after the entry in force that selects `target`; `None` past the
/// last.
fn next_seq(active: Option<InForce>, target: Slot) -> Option<u32> {
    let mut seq = active.map_or(Some(1), |a| a.seq.checked_add(1));
    if seq.is_some_and(|seq| Slot::of_seq(seq) != target) {
        seq = seq.and_then(|seq| seq.checked_add(1));
    }
    seq.filter(|&seq| seq != SEQ_ERASED)
}

/// A complete entry selecting `seq`, valid, its label erased.
fn valid_entry(seq: u32) -> Entry {
    let mut entry = [0xFFu8; ENTRY];
    for (at, value) in [(0, seq), (24, STATE_VALID), (28, seq_crc(seq))] {
        if let Some(field) = entry.get_mut(at..at + 4) {
            field.copy_from_slice(&value.to_le_bytes());
        }
    }
    entry
}
