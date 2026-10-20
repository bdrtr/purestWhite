#include "StreamingJsonParser.h"

#include <cstddef>

// bb_json_callbacks has JsonCallbacks' layout, so a caller's callbacks pass through unchanged.
static_assert(sizeof(bb_json_callbacks) == sizeof(JsonCallbacks), "callback layout");
static_assert(offsetof(bb_json_callbacks, on_key) == offsetof(JsonCallbacks, onKey), "callback layout");
static_assert(offsetof(bb_json_callbacks, on_bool) == offsetof(JsonCallbacks, onBool), "callback layout");
static_assert(offsetof(bb_json_callbacks, on_array_end) == offsetof(JsonCallbacks, onArrayEnd), "callback layout");

StreamingJsonParser::StreamingJsonParser(const JsonCallbacks& cb)
    : callbacks{cb.ctx,    cb.onKey,         cb.onString,    cb.onNumber,     cb.onBool,
                cb.onNull, cb.onObjectStart, cb.onObjectEnd, cb.onArrayStart, cb.onArrayEnd} {
  bb_json_init(&state);
}

void StreamingJsonParser::reset() { bb_json_reset(&state); }

void StreamingJsonParser::feed(const char* data, size_t len) { bb_json_feed(&state, &callbacks, data, len); }

bool StreamingJsonParser::hasError() const { return bb_json_has_error(&state); }
