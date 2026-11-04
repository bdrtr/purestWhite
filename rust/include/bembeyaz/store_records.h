// C ABI of rust/crates/bembeyaz-ffi/src/store_records.rs: the small stores whose files are a list
// of records of texts, or one such record (OpdsTokenStore, LcpPassphraseStore, LcpDeviceStore).
// Use those stores' headers instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// The stores (bembeyaz_store::records::Store), and each one's fields in their order: the *_OBF
// fields as a file holds them (obfuscated); a record loaded (bb_store_records_load) holds them plain,
// at the same places, named without _OBF.
enum {
  BB_STORE_RECORDS_OPDS_TOKENS = 0,      // opds_tokens.json, the list "tokens"
  BB_STORE_RECORDS_LCP_PASSPHRASES = 1,  // lcp_passphrases.json, the list "passphrases"
  BB_STORE_RECORDS_LCP_DEVICE = 2,       // lcp_device.json, one record (the root object)
};
enum {
  BB_STORE_OPDS_TOKEN_URL = 0,
  BB_STORE_OPDS_TOKEN_ACCESS_OBF = 1,
  BB_STORE_OPDS_TOKEN_REFRESH_OBF = 2,
  BB_STORE_OPDS_TOKEN_REFRESH_URL = 3,
  BB_STORE_OPDS_TOKEN_FIELDS = 4,
  BB_STORE_OPDS_TOKEN_ACCESS = BB_STORE_OPDS_TOKEN_ACCESS_OBF,    // loaded: plain
  BB_STORE_OPDS_TOKEN_REFRESH = BB_STORE_OPDS_TOKEN_REFRESH_OBF,  // loaded: plain
};
enum {
  BB_STORE_LCP_PASSPHRASE_PROVIDER = 0,
  BB_STORE_LCP_PASSPHRASE_KEY_OBF = 1,
  BB_STORE_LCP_PASSPHRASE_FIELDS = 2,
  BB_STORE_LCP_PASSPHRASE_KEY = BB_STORE_LCP_PASSPHRASE_KEY_OBF,  // loaded: plain
};
enum {
  BB_STORE_LCP_DEVICE_ID = 0,
  BB_STORE_LCP_DEVICE_PRIV_OBF = 1,
  BB_STORE_LCP_DEVICE_FIELDS = 2,
  BB_STORE_LCP_DEVICE_PRIV = BB_STORE_LCP_DEVICE_PRIV_OBF,  // loaded: plain
};

// The limits the stores keep to, Rust's (bembeyaz_store::records): OpdsTokenStore::MAX_SERVERS,
// LcpPassphraseStore::MAX_PROVIDERS, the longest token and hash decoded.
#define BB_STORE_RECORDS_MOST_OPDS_TOKENS 8
#define BB_STORE_RECORDS_MOST_LCP_PASSPHRASES 8
#define BB_STORE_RECORDS_MOST_TOKEN 2048
#define BB_STORE_RECORDS_MOST_HASH 64

// Receives one record a store keeps (its fields in the store's order, the secret ones plain), and
// how many it keeps; the texts are valid during the call only. It runs inside Rust: it must
// return normally (no exception, no longjmp).
typedef void (*bb_store_record_read)(const bb_store_text* fields, size_t count, void* context);

// Hands write the store's text for the count records whose fields are at fields (the store's
// number of fields each, one record after another; BB_STORE_RECORDS_LCP_DEVICE writes one), in one call; false when
// it could not be made (memory ran out, or store is none of the above) and write was not called.
bool bb_store_records_write(int store, const bb_store_text* fields, size_t count, bb_store_write write, void* context);
// Reads the store's text (len bytes, read up to a NUL as a C string) and hands record each record
// the store keeps, in order (bembeyaz_store::records::load: the *_OBF fields decoded with decode,
// empty when they do not decode or are too long; an OPDS token entry kept when its URL is not
// empty, an LCP passphrase when its provider and hash are not; at most the limits above); decode
// and record are both given context. NULL when it was read, else ArduinoJson's name for why not (a
// static string; "NoMemory" when memory ran out, decoding included; InvalidInput for a store that
// is none of the above), and only decode may have been called. Rust's copies of the plain secrets
// are wiped after the calls; the ones record makes are the caller's.
const char* bb_store_records_load(const char* text, size_t len, int store, bb_store_decode decode,
                                  bb_store_record_read record, void* context);

#ifdef __cplusplus
}
#endif
