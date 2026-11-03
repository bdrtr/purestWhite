//! Exports for the stores of records of texts (see `rust/include/bembeyaz/store_records.h`):
//! their files read and written by `bembeyaz_store::records`. Read records reach the caller one
//! per callback call, and written text through a `bb_store_write` callback, so no Rust buffer
//! outlives a call.

use crate::store::{Text, Write, hand_over};
use alloc::vec::Vec;
use bembeyaz_store::records::{
    LCP_DEVICE_KEYS, LCP_PASSPHRASE_KEYS, OPDS_TOKEN_KEYS, Store, read_each, write,
};
use core::ffi::{c_char, c_int, c_void};

// store_records.h's field counts: BB_STORE_OPDS_TOKEN_FIELDS and the two others.
const _: () = assert!(OPDS_TOKEN_KEYS.len() == 4);
const _: () = assert!(LCP_PASSPHRASE_KEYS.len() == 2 && LCP_DEVICE_KEYS.len() == 2);

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

/// Reads the store's text; NULL when it was read (each record handed to `record`, with how many
/// there are), else ArduinoJson's name for why not (`InvalidInput` for a `store` that is none of
/// the stores), and `record` is not called.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `record` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_records_read(
    text: *const c_char,
    len: usize,
    store: c_int,
    record: Option<ReadRecord>,
    context: *mut c_void,
) -> *const c_char {
    let Some(shape) = store_of(store).map(Store::shape) else {
        return c"InvalidInput".as_ptr();
    };
    // SAFETY: as documented.
    let text = unsafe { crate::view(text.cast::<u8>(), len) };
    let handed = read_each(text, &shape, |fields, count| {
        if let Some(record) = record {
            let texts: Vec<Text> = fields.iter().map(|f| Text::of(f)).collect();
            // SAFETY: `texts` and the fields it views are readable for the call; the caller
            // vouches for record and context.
            unsafe { record(texts.as_ptr(), count, context) };
        }
    });
    match handed {
        Ok(()) => core::ptr::null(),
        Err(error) => error.c_name().as_ptr(),
    }
}
