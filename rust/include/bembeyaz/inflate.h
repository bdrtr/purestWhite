// C ABI of rust/crates/bembeyaz-ffi/src/inflate.rs. Use lib/InflateStream/InflateStream.h instead.
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Read results: BB_INFLATE_MORE (dest is full, more remains), BB_INFLATE_DONE,
// or a negative error (bb_inflate_status_str describes it).
enum {
  BB_INFLATE_MORE = 0,
  BB_INFLATE_DONE = 1,
};

enum { BB_INFLATE_WINDOW_SIZE = 32768 };

// Lives in caller-provided memory and needs no destructor: free the blocks.
typedef struct bb_inflate bb_inflate;

size_t bb_inflate_state_size(void);
size_t bb_inflate_state_align(void);
// Sets up a raw DEFLATE stream in state. window is BB_INFLATE_WINDOW_SIZE bytes
// of any contents, or NULL for one-shot mode. Returns state, or NULL when state
// is NULL, misaligned or too small, or window is too small.
bb_inflate* bb_inflate_init(void* state, size_t state_len, uint8_t* window, size_t window_len);
// The whole input, valid for the life of the stream.
void bb_inflate_set_source(bb_inflate* stream, const uint8_t* data, size_t len);
// fill points *data at the next input bytes and returns their count (valid
// until the next call); 0 ends the input.
void bb_inflate_set_fill(bb_inflate* stream, size_t (*fill)(void* context, const uint8_t** data), void* context);
// zlib header + Adler-32 trailer; call before the first read.
void bb_inflate_set_zlib(bb_inflate* stream);
// Fills dest until max bytes or the end of the stream; *produced counts the
// bytes written, even on an error. In one-shot mode each dest must directly
// follow the previous read's output in the same buffer.
int bb_inflate_read(bb_inflate* stream, uint8_t* dest, size_t max, size_t* produced);
const char* bb_inflate_status_str(int status);

#ifdef __cplusplus
}
#endif
