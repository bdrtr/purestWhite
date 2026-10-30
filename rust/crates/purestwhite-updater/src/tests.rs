// The recovery mode's scenarios on the host: a card in memory formatted by the host's mkfs.fat
// (without it the tests fail, unless PW_FAT_SKIP_TOOLS is set: then they say so and pass), the
// release written on it through purestwhite-fat, and the C++ firmware's slot in a NOR flash in
// memory. The fakes index and unwrap directly: a wrong result fails the test.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

mod fixtures;
mod images;
mod scenarios;

/// The C++ firmware's slot in the tests: room for the test releases, not for an image of its size.
const SLOT: usize = 0x4_0000;
