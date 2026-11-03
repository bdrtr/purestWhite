//! The firmware's settings files (`/.crosspoint/*.json`, written by the stores built on
//! `lib/Serialization/PersistableStore.h`) read and written as the C++ stores did with
//! ArduinoJson: the same JSON out, the same values from any file, a field of the wrong type
//! falling back to its default. One module per store, each with `read` and `write`; each one's
//! golden corpus is that store's C++ answers.
//!
//! - [`state`]: `CrossPointState` (`state.json`): the open book, the recent sleep images; corpus
//!   from `test/crosspoint_state/make_corpus.py`.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod state;
#[cfg(test)]
mod state_tests;

pub use bembeyaz_jsondoc::Error;
