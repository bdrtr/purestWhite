// SPDX-FileCopyrightText: 2021-2022 Espressif Systems (Shanghai) CO LTD
// SPDX-License-Identifier: Apache-2.0
//
// The tinfl part of ESP-IDF's components/esp_rom/include/miniz.h, cut down to what InflateStream
// uses and written with fixed-width types: the ESP32-S3 mask ROM's miniz 1.15 inflate. That header
// cannot be included by name: freeink-sdk's ContentProtection/library.json adds
// -I third_party/miniz, so <miniz.h> finds its miniz 3.x, whose tinfl_decompressor has another
// layout, and the ROM function would write past a block of the wrong size. Host tests build
// test/rom_tinfl (miniz 1.15's source) against this header.
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
  TINFL_FLAG_PARSE_ZLIB_HEADER = 1,
  TINFL_FLAG_HAS_MORE_INPUT = 2,
  TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF = 4,
  TINFL_FLAG_COMPUTE_ADLER32 = 8,
};

typedef enum {
  TINFL_STATUS_BAD_PARAM = -3,
  TINFL_STATUS_ADLER32_MISMATCH = -2,
  TINFL_STATUS_FAILED = -1,
  TINFL_STATUS_DONE = 0,
  TINFL_STATUS_NEEDS_MORE_INPUT = 1,
  TINFL_STATUS_HAS_MORE_OUTPUT = 2,
} tinfl_status;

#define TINFL_LZ_DICT_SIZE 32768

enum {
  TINFL_MAX_HUFF_TABLES = 3,
  TINFL_MAX_HUFF_SYMBOLS_0 = 288,
  TINFL_MAX_HUFF_SYMBOLS_1 = 32,
  TINFL_MAX_HUFF_SYMBOLS_2 = 19,
  TINFL_FAST_LOOKUP_BITS = 10,
  TINFL_FAST_LOOKUP_SIZE = 1 << TINFL_FAST_LOOKUP_BITS,
};

typedef struct {
  uint8_t m_code_size[TINFL_MAX_HUFF_SYMBOLS_0];
  int16_t m_look_up[TINFL_FAST_LOOKUP_SIZE], m_tree[TINFL_MAX_HUFF_SYMBOLS_0 * 2];
} tinfl_huff_table;

struct tinfl_decompressor_tag {
  uint32_t m_state, m_num_bits, m_zhdr0, m_zhdr1, m_z_adler32, m_final, m_type, m_check_adler32, m_dist, m_counter,
      m_num_extra, m_table_sizes[TINFL_MAX_HUFF_TABLES];
  uint32_t m_bit_buf;
  size_t m_dist_from_out_buf_start;
  tinfl_huff_table m_tables[TINFL_MAX_HUFF_TABLES];
  uint8_t m_raw_header[4], m_len_codes[TINFL_MAX_HUFF_SYMBOLS_0 + TINFL_MAX_HUFF_SYMBOLS_1 + 137];
};
typedef struct tinfl_decompressor_tag tinfl_decompressor;

#define tinfl_init(r) \
  do {                \
    (r)->m_state = 0; \
  } while (0)

// On the device, linked to the ROM by esp32s3.rom.ld.
tinfl_status tinfl_decompress(tinfl_decompressor* r, const uint8_t* pIn_buf_next, size_t* pIn_buf_size,
                              uint8_t* pOut_buf_start, uint8_t* pOut_buf_next, size_t* pOut_buf_size,
                              const uint32_t decomp_flags);

#ifdef __cplusplus
}

// Its size under esp_rom/include/miniz.h on the S3 (size_t is wider on a 64-bit host).
static_assert(sizeof(void*) != 4 || sizeof(tinfl_decompressor) == 10992, "not the ROM's tinfl_decompressor");
#endif
