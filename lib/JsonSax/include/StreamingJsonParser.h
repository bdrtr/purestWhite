#pragma once

// The FreeInk SDK's streaming JSON parser API (freeink-sdk/libs/network/JsonSax at 084fdfb),
// implemented by this fork's Rust reader (rust/crates/bembeyaz-json). The SDK's OPDS, catalog and
// LCP readers use it unchanged.

#include <bembeyaz/json.h>

#include <cstddef>
#include <cstdint>

struct JsonCallbacks {
  void* ctx;
  void (*onKey)(void* ctx, const char* key, size_t len);
  void (*onString)(void* ctx, const char* value, size_t len);
  void (*onNumber)(void* ctx, const char* value, size_t len);
  void (*onBool)(void* ctx, bool value);
  void (*onNull)(void* ctx);
  void (*onObjectStart)(void* ctx);
  void (*onObjectEnd)(void* ctx);
  void (*onArrayStart)(void* ctx);
  void (*onArrayEnd)(void* ctx);
};

class StreamingJsonParser {
 public:
  // The token buffer, NUL included: a decoded key, string or number of up to
  // TOKEN_BUF_SIZE - 1 bytes; a longer one is dropped whole, not cut. It covers
  // the OPDS readers' URLs (768-char hrefs/templates, see OpdsLimits); a
  // description of 2048 bytes (the detail parser's limit) or more is dropped.
  static constexpr size_t TOKEN_BUF_SIZE = BB_JSON_TOKEN_BUF_SIZE;
  static constexpr size_t MAX_NESTING = BB_JSON_MAX_NESTING;

  explicit StreamingJsonParser(const JsonCallbacks& callbacks);
  StreamingJsonParser(const StreamingJsonParser&) = delete;
  StreamingJsonParser& operator=(const StreamingJsonParser&) = delete;

  void reset();
  void feed(const char* data, size_t len);

  bool hasError() const;

 private:
  bb_json_callbacks callbacks;
  bb_json_parser state;
};
