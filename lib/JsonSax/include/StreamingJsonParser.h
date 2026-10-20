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
  // TOKEN_BUF_SIZE - 1 bytes; it covers the OPDS readers' URLs (768-char hrefs
  // and templates, see OpdsLimits). A longer string value is reported cut to at
  // most that, ending on a whole UTF-8 character, so a long OPDS description
  // shows its start instead of nothing. Readers that keep a value whole (the
  // OAuth token reader, the plugin catalog) then get a cut token or URL that
  // fails later, where a dropped one failed at once. A longer key or number is
  // still dropped whole.
  static constexpr size_t TOKEN_BUF_SIZE = BB_JSON_TOKEN_BUF_SIZE;
  static constexpr size_t MAX_NESTING = BB_JSON_MAX_NESTING;

  explicit StreamingJsonParser(const JsonCallbacks& callbacks);
  StreamingJsonParser(const StreamingJsonParser&) = delete;
  StreamingJsonParser& operator=(const StreamingJsonParser&) = delete;

  void reset();
  void feed(const char* data, size_t len);

  // On by default (reset() keeps it); off drops a long string value whole, as the
  // SDK's parser did.
  void setTruncateLongStrings(bool truncate);

  bool hasError() const;

 private:
  bb_json_callbacks callbacks;
  bb_json_parser state;
};
