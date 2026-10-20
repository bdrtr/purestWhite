// C ABI of rust/crates/bembeyaz-ffi/src/container.rs. Use lib/Epub/Epub/parsers/ContainerParser.h
// instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bb_container bb_container;

// A reader for META-INF/container.xml; NULL when out of memory. limit bounds the XML parser
// (BB_XML_DEFAULT_LIMIT).
bb_container* bb_container_new(size_t limit);
void bb_container_free(bb_container* container);
// Reads the next piece; last when nothing follows. A BB_XML_* code: BB_XML_OK, or why the
// document stopped (BB_XML_NO_MEMORY also when the path could not be held); later calls fail too.
int bb_container_parse(bb_container* container, const char* data, size_t len, bool last);
// The package document's path read so far (empty when none); valid until the next
// bb_container_parse or bb_container_free on this reader.
const char* bb_container_full_path(const bb_container* container, size_t* len);
// Whether the path changed since this was last called.
bool bb_container_take_path_changed(bb_container* container);
uint64_t bb_container_error_offset(const bb_container* container);

#ifdef __cplusplus
}
#endif
