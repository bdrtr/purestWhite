#pragma once
#include <cstddef>

// HTML named character references; the table is in rust/crates/bembeyaz-html.

// Lookup a single HTML entity (including & and ;) and return its UTF-8 value
// Returns nullptr if entity is not found
const char* lookupHtmlEntity(const char* entity, size_t len);

// For XmlParser's skipped entity handler: passes the entity to `text` as the UTF-8 value of an HTML
// entity, anything else as written (&name;).
void htmlEntityText(const char* name, void* userData, void (*text)(void* userData, const char* s, int len));
