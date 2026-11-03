#pragma once

#include <Arduino.h>
#include <bembeyaz/store.h>

#include <cstddef>
#include <string>

// What the stores whose JSON is Rust's share at the C ABI (rust/include/bembeyaz/store.h).

static_assert(sizeof(bb_store_text) == 2 * sizeof(void*) && offsetof(bb_store_text, len) == sizeof(void*),
              "bb_store_text layout");

// The JSON text Rust writes, appended to json; ok turns false when the String cannot grow.
struct StoreJsonSink {
  String& json;
  bool ok;
};

// A bb_store_write that appends to a StoreJsonSink.
inline void appendStoreJson(const char* data, const size_t len, void* context) {
  auto* sink = static_cast<StoreJsonSink*>(context);
  sink->ok = sink->ok && sink->json.concat(data, static_cast<unsigned int>(len));
}

// A view of s, valid while s is not changed.
inline bb_store_text storeText(const std::string& s) { return bb_store_text{s.data(), s.size()}; }

// A copy of t's bytes.
inline std::string storeString(const bb_store_text& t) { return t.len ? std::string(t.data, t.len) : std::string(); }
