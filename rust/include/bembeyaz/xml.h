// C ABI of rust/crates/bembeyaz-ffi/src/xml.rs. Use lib/XmlParserUtils/XmlParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bb_xml_parser bb_xml_parser;

// What a document reports. Strings end with a NUL and live until the callback returns; atts
// holds name, value, name, value, ..., NULL; offset is where the start tag begins in the input.
// Text may come in several pieces. Each callback returns false to stop parsing (the parse then
// fails with BB_XML_ABORTED). Leave a callback NULL to ignore its event.
typedef struct {
  bool (*start_element)(void* context, const char* name, const char** atts, uint64_t offset);
  bool (*end_element)(void* context, const char* name);
  bool (*text)(void* context, const char* text, size_t len);
  bool (*comment)(void* context, const char* text);
  bool (*processing_instruction)(void* context, const char* target, const char* data);
  bool (*cdata_start)(void* context);
  bool (*cdata_end)(void* context);
  // A reference to an entity an external DTD subset may declare (it is not read), or after
  // bb_xml_skip_undefined_entities, to any entity nobody declared.
  bool (*skipped_entity)(void* context, const char* name);
} bb_xml_handlers;

// Error codes are expat's XML_Error values.
enum {
  BB_XML_OK = 0,
  BB_XML_NO_MEMORY = 1,
  BB_XML_ABORTED = 35,
  BB_XML_FINISHED = 36,
};

// The limit XmlParser.h and the expat API use. Expat could not have held a larger token either
// (it doubles its buffer, to 8 MiB past this size), and half the 8 MB PSRAM stays free.
#define BB_XML_DEFAULT_LIMIT ((size_t)4 * 1024 * 1024)

// A parser that fails with BB_XML_NO_MEMORY rather than hold more than `limit` bytes for one
// token, the open element names or the DTD; NULL when out of memory. Free it with bb_xml_free.
bb_xml_parser* bb_xml_new(size_t limit);
void bb_xml_free(bb_xml_parser* parser);

// Parses the next len bytes of the document; `last` when nothing follows. Returns 0 or an error
// code. After an error, every later call returns the same code; after `last`, BB_XML_FINISHED.
int bb_xml_parse(bb_xml_parser* parser, const char* data, size_t len, bool last, const bb_xml_handlers* handlers,
                 void* context);

// Reports references to entities nobody declared to skipped_entity (in attribute values they read
// as nothing) instead of failing, even in a standalone document. Call before parsing.
void bb_xml_skip_undefined_entities(bb_xml_parser* parser);

// Where in the input the token that failed starts, in bytes.
uint64_t bb_xml_error_offset(const bb_xml_parser* parser);

// A static description of an error code.
const char* bb_xml_error_message(int code);

#ifdef __cplusplus
}
#endif
