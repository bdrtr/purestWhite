// C ABI of rust/crates/bembeyaz-ffi/src/html.rs (rust/crates/bembeyaz-html).
// Use lib/Epub/Epub/htmlEntities.h instead.
#pragma once

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// The UTF-8 value of the HTML entity name (len bytes, without '&' and ';'), as a static C string;
// NULL when there is none. Case-sensitive.
const char* bb_html_entity(const char* name, size_t len);

#ifdef __cplusplus
}
#endif
