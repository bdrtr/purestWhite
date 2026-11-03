// C ABI of rust/crates/bembeyaz-ffi/src/kosync_messages.rs. Use lib/KOReaderSync/KOReaderSyncMessages.h
// instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Bytes (not NUL-terminated; they may hold a NUL).
typedef struct {
  const char* data;
  size_t len;
} bb_kosync_text;

// Callbacks run inside Rust: they must return normally (no exception, no longjmp).

// Appends len bytes to the caller's output.
typedef void (*bb_kosync_write)(const char* data, size_t len, void* context);

// A progress update (bembeyaz_kosync::messages::Update).
typedef struct {
  bb_kosync_text document;
  bb_kosync_text progress;
  float percentage;
  bool has_metadata;
  bb_kosync_text filename;
  bb_kosync_text title;
  bb_kosync_text authors;
  bb_kosync_text extra_json;
  bb_kosync_text isbn;
  bb_kosync_text asin;
  bb_kosync_text series;
  bool has_series_index;
  float series_index;
  bool has_position;
  uint32_t pct_q;
  uint16_t spine;
  uint16_t page;
  uint16_t pages;
  bool has_para;
  uint16_t para;
  bb_kosync_text xpath;
} bb_kosync_update;

// Writes POST /users/create's body.
void bb_kosync_create_user_body(bb_kosync_text username, bb_kosync_text md5_password, bb_kosync_write write,
                                void* context);
// Writes PUT /syncs/progress's body; extended_metadata adds the bibliographic fields, rich_progress the
// CrossPoint position.
void bb_kosync_progress_body(const bb_kosync_update* update, bool extended_metadata, bool rich_progress,
                             bb_kosync_write write, void* context);

// The text fields of a progress answer, handed to bb_kosync_parse_progress's field callback in this
// order (XPATH only when has_position) and only when the answer was read.
enum {
  BB_KOSYNC_FIELD_PROGRESS = 0,
  BB_KOSYNC_FIELD_DEVICE = 1,
  BB_KOSYNC_FIELD_DEVICE_ID = 2,
  BB_KOSYNC_FIELD_XPATH = 3,
};

// Writes one text field of a progress answer.
typedef void (*bb_kosync_field)(int field, const char* data, size_t len, void* context);

// The other fields of a progress answer.
typedef struct {
  float percentage;
  int64_t timestamp;
  bool has_position;
  uint32_t pct_q;
  uint16_t spine;
  uint16_t page;
  uint16_t pages;
  bool has_para;
  uint16_t para;
} bb_kosync_received;

// Reads GET /syncs/progress's answer (len bytes, read up to a NUL as a C string). NULL when it was
// read (its text fields handed to field, the rest in out), else ArduinoJson's name for why not (a
// static string).
const char* bb_kosync_parse_progress(const char* body, size_t len, bool rich_progress, bb_kosync_received* out,
                                     bb_kosync_field field, void* context);

#ifdef __cplusplus
}
#endif
