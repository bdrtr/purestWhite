#pragma once

// A book's clipping store (rust/crates/bembeyaz-clippings): the store file's header, records and
// texts, and the deletion journal's ids. src/ClippingStore keeps the files (the backup, the
// temporary file and the renames), the clippings in memory, the writing of the journal and the
// making of ids.

#include <bembeyaz/io.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BB_CLIP_TEXT_MAX 4096
#define BB_CLIP_ID_LEN 65
#define BB_CLIP_CHAPTER_LEN 48

// A clipping's record: where it is in the book, its sync state, and its text's offset and length
// in the store file. id and chapter end in a zero.
typedef struct {
  uint16_t spine;
  uint16_t start_page;
  uint16_t end_page;
  uint16_t page_count;
  uint16_t start_word;
  uint16_t end_word;
  uint16_t word_count;
  uint16_t paragraph;
  uint32_t timestamp;
  uint32_t layout;
  uint32_t start_offset;
  uint32_t end_offset;
  uint64_t sync_revision;
  char id[BB_CLIP_ID_LEN];
  bool pending_upload;
  char chapter[BB_CLIP_CHAPTER_LEN];
  uint32_t text_offset;
  uint16_t text_length;
} bb_clip_record;

// The header's strings.
enum {
  BB_CLIP_TITLE = 0,
  BB_CLIP_AUTHOR = 1,
  BB_CLIP_PATH = 2,
};

// Takes what a load finds: text gives room for a header string's len bytes (NULL skips it),
// begin is told the record count (false when there is no room for them), then record is given
// each record. A field the file's version does not have has its default (no layout signature,
// no source offsets, not uploaded, no id).
typedef struct {
  void* context;
  char* (*text)(void* context, uint8_t field, size_t len);
  bool (*begin)(void* context, uint16_t count);
  void (*record)(void* context, const bb_clip_record* record);
} bb_clip_target;

enum {
  BB_CLIP_LOADED = 0,
  // An unknown version, a cut file, or a count, string or text over its limit.
  BB_CLIP_DAMAGED = 1,
  BB_CLIP_NO_ROOM = 2,
};

// Reads the store file (versions 1 to 4) through buf (buf_len bytes) into target; max_count is
// the most clippings a book holds. Returns a BB_CLIP_ code; target may have taken records before
// a failure.
int bb_clip_load(const bb_io_file* file, uint8_t* buf, size_t buf_len, uint16_t max_count,
                 const bb_clip_target* target);

// Gives record index (with its text's offset and length in the old store file) for a save.
typedef struct {
  void* context;
  void (*record)(void* context, uint16_t index, bb_clip_record* out);
} bb_clip_records;

// Writes the store file (version 4) to out from offset 0: the header strings, then count records
// with their texts. Record replacement's text is text (text_len bytes, at most BB_CLIP_TEXT_MAX
// kept) unless text is NULL; every other text is copied from source (the old store file, NULL for
// none). offsets (count values) takes each text's offset in the new file. False when an argument
// is NULL, a header string is over BB_CLIP_TEXT_MAX, or a write or a copy fails.
bool bb_clip_save(const bb_io_sink* out, const bb_io_file* source, const char* const header[3],
                  const size_t header_len[3], uint16_t count, const bb_clip_records* records, uint16_t replacement,
                  const char* text, size_t text_len, uint32_t* offsets);

// The next id from *offset in the deletion journal that known (an id still in the store) does
// not know, with *offset past it; an empty id at the journal's end. False when *offset is not a
// multiple of 65 within the journal, or an id cannot be read.
bool bb_clip_next_deletion(const bb_io_file* journal, uint32_t* offset, char id[BB_CLIP_ID_LEN], void* context,
                           bool (*known)(void* context, const char* id));

#ifdef __cplusplus
}
#endif
