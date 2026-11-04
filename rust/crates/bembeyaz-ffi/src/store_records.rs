//! Exports for the stores of records of texts (see `rust/include/bembeyaz/store_records.h`):
//! their files read, made into the records each store keeps (their secrets decoded through the
//! caller's callback), and written by `bembeyaz_store::records`. The records reach the caller one
//! per callback call, and written text through a `bb_store_write` callback, so no Rust buffer
//! outlives a call.

use crate::store::{Decode, Text, Write, decoder, hand_over};
use alloc::vec::Vec;
use bembeyaz_store::records::{
    LCP_DEVICE_ID, LCP_DEVICE_KEYS, LCP_DEVICE_PRIV, LCP_PASSPHRASE_KEY, LCP_PASSPHRASE_KEYS,
    LCP_PASSPHRASE_PROVIDER, MOST_FIELDS, MOST_HASH, MOST_LCP_PASSPHRASES, MOST_OPDS_TOKENS,
    MOST_TOKEN, OPDS_TOKEN_ACCESS, OPDS_TOKEN_KEYS, OPDS_TOKEN_REFRESH, OPDS_TOKEN_REFRESH_URL,
    OPDS_TOKEN_URL, Store, load, write,
};
use core::ffi::{c_char, c_int, c_void};

// store_records.h's field counts: BB_STORE_OPDS_TOKEN_FIELDS and the two others.
const _: () = assert!(OPDS_TOKEN_KEYS.len() == 4);
const _: () = assert!(LCP_PASSPHRASE_KEYS.len() == 2 && LCP_DEVICE_KEYS.len() == 2);
// store_records.h's limits.
const _: () = assert!(MOST_OPDS_TOKENS == 8 && MOST_LCP_PASSPHRASES == 8);
const _: () = assert!(MOST_TOKEN == 2048 && MOST_HASH == 64);
// store_records.h's field numbers.
const _: () = assert!(OPDS_TOKEN_URL == 0 && OPDS_TOKEN_ACCESS == 1 && OPDS_TOKEN_REFRESH == 2);
const _: () = assert!(OPDS_TOKEN_REFRESH_URL == 3 && LCP_PASSPHRASE_PROVIDER == 0);
const _: () = assert!(LCP_PASSPHRASE_KEY == 1 && LCP_DEVICE_ID == 0 && LCP_DEVICE_PRIV == 1);

/// store_records.h's `bb_store_record_read`.
type ReadRecord = unsafe extern "C" fn(*const Text, usize, *mut c_void);

/// store_records.h's `BB_STORE_RECORDS_*`.
fn store_of(code: c_int) -> Option<Store> {
    [Store::OpdsTokens, Store::LcpPassphrases, Store::LcpDevice]
        .into_iter()
        .find(|s| c_int::from(*s as u8) == code)
}

// store_records.h's BB_STORE_RECORDS_OPDS_TOKENS, _LCP_PASSPHRASES and _LCP_DEVICE.
const _: () = assert!(
    Store::OpdsTokens as u8 == 0 && Store::LcpPassphrases as u8 == 1 && Store::LcpDevice as u8 == 2
);

/// Hands `write_text` the store's text for the `count` records at `fields`, in one call; false
/// when it could not be made (memory ran out, or `store` is none of the stores) and `write_text`
/// was not called.
///
/// # Safety
/// `fields` is NULL or `count` times the store's number of fields texts, each NULL or
/// readable; `write_text` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_records_write(
    store: c_int,
    fields: *const Text,
    count: usize,
    write_text: Option<Write>,
    context: *mut c_void,
) -> bool {
    let Some(shape) = store_of(store).map(Store::shape) else {
        return false;
    };
    let total = count.saturating_mul(shape.keys.len());
    // SAFETY: as documented.
    let fields: Vec<&[u8]> = unsafe { crate::view(fields, total) }
        .iter()
        // SAFETY: as documented.
        .map(|f| unsafe { f.bytes() })
        .collect();
    // SAFETY: as documented.
    unsafe { hand_over(write(&shape, &fields).as_deref(), write_text, context) }
}

/// Reads the store's text and hands `record` each record the store keeps (`bembeyaz_store::
/// records::load`: its secret fields, the `*_OBF` ones, decoded with `decode`), in order, with how
/// many there are. NULL when it was read, else ArduinoJson's name for why not ("NoMemory" when
/// memory ran out, decoding included; `InvalidInput` for a `store` that is none of the stores),
/// and only `decode` may have been called. Rust's copies of the plain secrets are wiped after the
/// calls; the ones `record` makes are the caller's.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `decode` and `record` take `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_records_load(
    text: *const c_char,
    len: usize,
    store: c_int,
    decode: Option<Decode>,
    record: Option<ReadRecord>,
    context: *mut c_void,
) -> *const c_char {
    let Some(store) = store_of(store) else {
        return c"InvalidInput".as_ptr();
    };
    // SAFETY: as documented.
    let (text, decode) = unsafe {
        (
            crate::view(text.cast::<u8>(), len),
            decoder(decode, context),
        )
    };
    let kept = match load(store, text, decode) {
        Ok(kept) => kept,
        Err(error) => return error.c_name().as_ptr(),
    };
    if let Some(record) = record {
        for fields in &kept {
            let mut texts = [Text::of(&[]); MOST_FIELDS];
            for (text, field) in texts.iter_mut().zip(fields) {
                *text = Text::of(field);
            }
            // SAFETY: `texts` and the fields it views are readable for the call; the caller
            // vouches for record and context.
            unsafe { record(texts.as_ptr(), kept.len(), context) };
        }
    }
    // `kept`'s plain secrets are wiped as it is dropped.
    core::ptr::null()
}
