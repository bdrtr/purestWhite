//! What the SD updater may write while the slot it writes holds the known-good C++ firmware
//! (phase 2, rust/firmware/DESIGN.md): only a release it knows, and only over the same release,
//! which proves the write path on the real slot and changes nothing, or over a slot whose image no
//! longer passes and which the bootloader has just refused (a write cut short). A slot holding
//! another image that passes is left alone. [`resets`] says whether the updater then resets into
//! the slot or waits (a sleep), so that it never resets straight back into itself.
//!
//! An image passes when its segments, chip, app descriptor, checksum and appended SHA-256 hold
//! ([`crate::image`], [`crate::digest`]): what the bootloader checks before it boots an image.

use crate::image::DIGEST;

#[cfg(test)]
mod tests;

/// What the slot to be written holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InSlot {
    /// An image that passes, with this digest.
    Image([u8; DIGEST]),
    /// No image that passes, as read.
    Broken,
    /// The flash could not be read.
    Unreadable,
}

/// What the updater does with the card's image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Writes it over the same image.
    Rewrite,
    /// Writes it over a slot that holds no image that passes, which the bootloader refused.
    Repair,
    Refuse(Refusal),
}

/// Why the updater writes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The card's image is not a release the updater knows.
    Unknown,
    /// The slot holds another image that passes.
    Another,
    /// The slot could not be read.
    Unreadable,
    /// The slot reads as holding no image that passes, but the bootloader did not refuse it: the
    /// read is not trusted over the bootloader's own check.
    Unconfirmed,
    /// The slot reads as holding an image that passes, but the bootloader refused it: writing the
    /// same bytes again would not change its check.
    Disagree,
}

/// The plan for the card's image, whose digest is `card` (a `known` release or not), and the slot.
/// `fell_back`: the bootloader started the updater although otadata selects the slot, so it found
/// no image there it would boot. Only then is a slot read as broken written over, so that a fault
/// in reading the flash cannot turn a slot the bootloader boots into one it does not.
#[must_use]
pub fn plan(card: &[u8; DIGEST], known: bool, slot: InSlot, fell_back: bool) -> Plan {
    if !known {
        return Plan::Refuse(Refusal::Unknown);
    }
    match slot {
        InSlot::Image(_) if fell_back => Plan::Refuse(Refusal::Disagree),
        InSlot::Image(digest) if digest == *card => Plan::Rewrite,
        InSlot::Image(_) => Plan::Refuse(Refusal::Another),
        InSlot::Broken if fell_back => Plan::Repair,
        InSlot::Broken => Plan::Refuse(Refusal::Unconfirmed),
        InSlot::Unreadable => Plan::Refuse(Refusal::Unreadable),
    }
}

/// Whether the updater, its work done, resets into the slot it writes: the slot holds an image
/// that passes (`after`), and either the bootloader did not refuse the slot (`fell_back` false) or
/// the updater wrote that image itself (`wrote`). Otherwise it waits (a bounded sleep): a reset
/// would only bring the bootloader back to the updater's own image.
#[must_use]
pub fn resets(after: InSlot, wrote: bool, fell_back: bool) -> bool {
    matches!(after, InSlot::Image(_)) && (wrote || !fell_back)
}
