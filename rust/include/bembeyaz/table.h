// C ABI of rust/crates/bembeyaz-ffi/src/table.rs. Use lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h
// instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A grid table's pre-scan: per column, the longest word and the longest unwrapped line.
typedef struct {
  uint16_t min_width[4];
  uint16_t pref_width[4];
  uint8_t columns;
} bb_table_measure;

// Measures the table whose raw markup read hands out, from its <table tag on. read fills buf with up
// to len bytes and returns how many, 0 at the end; advance returns the width of a NUL-terminated
// word in the regular or bold style. Reads at most about 32 KB. False (and out, if not NULL, zeroed)
// when the table has fewer than two columns or more than four, does not end within that, or read or
// advance is NULL.
bool bb_table_measure_columns(size_t (*read)(void* context, uint8_t* buf, size_t len),
                              int (*advance)(void* context, const char* word, bool bold), void* context,
                              int space_width, bb_table_measure* out);
// Shares available across the measured columns the way CSS auto table layout does, with padding on
// each side of every cell, and writes measure->columns widths. False (widths untouched) when there
// are fewer than two or more than four columns, or a pointer is NULL.
bool bb_table_plan_columns(const bb_table_measure* measure, int available, int padding, uint16_t* widths);

#ifdef __cplusplus
}
#endif
