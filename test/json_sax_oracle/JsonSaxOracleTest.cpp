// The SDK's original StreamingJsonParser (renamed to SdkStreamingJsonParser by the build) and
// this fork's Rust one must report the same events and errors for any input, however it is split.
#include <StreamingJsonParser.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#define StreamingJsonParser SdkStreamingJsonParser
#define JsonCallbacks SdkJsonCallbacks
#include "../../freeink-sdk/libs/network/JsonSax/include/StreamingJsonParser.h"
#undef StreamingJsonParser
#undef JsonCallbacks

namespace {

struct Log {
  std::string text;
};

void add(void* ctx, char kind, const char* data, size_t len) {
  auto& log = static_cast<Log*>(ctx)->text;
  log += kind;
  log += std::to_string(len);
  log += ':';
  log.append(data, len);
  // Callers may read the token as a C string.
  log += data[len] == '\0' ? '|' : '!';
}
void onKey(void* ctx, const char* d, size_t n) { add(ctx, 'K', d, n); }
void onString(void* ctx, const char* d, size_t n) { add(ctx, 'S', d, n); }
void onNumber(void* ctx, const char* d, size_t n) { add(ctx, 'N', d, n); }
void onBool(void* ctx, bool v) { static_cast<Log*>(ctx)->text += v ? "T|" : "F|"; }
void onNull(void* ctx) { static_cast<Log*>(ctx)->text += "0|"; }
void onObjectStart(void* ctx) { static_cast<Log*>(ctx)->text += "{"; }
void onObjectEnd(void* ctx) { static_cast<Log*>(ctx)->text += "}"; }
void onArrayStart(void* ctx) { static_cast<Log*>(ctx)->text += "["; }
void onArrayEnd(void* ctx) { static_cast<Log*>(ctx)->text += "]"; }

// Each bit of mask keeps one callback; a cleared bit leaves it NULL, which skips its event.
template <typename Callbacks>
Callbacks callbacks(Log* log, uint32_t mask = ~0u) {
  Callbacks c{log, onKey, onString, onNumber, onBool, onNull, onObjectStart, onObjectEnd, onArrayStart, onArrayEnd};
  if (!(mask & 1)) c.onKey = nullptr;
  if (!(mask & 2)) c.onString = nullptr;
  if (!(mask & 4)) c.onNumber = nullptr;
  if (!(mask & 8)) c.onBool = nullptr;
  if (!(mask & 16)) c.onNull = nullptr;
  if (!(mask & 32)) c.onObjectStart = nullptr;
  if (!(mask & 64)) c.onObjectEnd = nullptr;
  if (!(mask & 128)) c.onArrayStart = nullptr;
  if (!(mask & 256)) c.onArrayEnd = nullptr;
  return c;
}

struct Rng {
  uint32_t state;
  uint32_t next() {
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    return state;
  }
  uint32_t below(uint32_t n) { return next() % n; }
};

void appendString(Rng& rng, std::string& out) {
  out += '"';
  const uint32_t len = rng.below(10) == 0 ? 2040 + rng.below(16) : rng.below(24);
  static const char* const pieces[] = {"\\\"",
                                       "\\\\",
                                       "\\/",
                                       "\\b",
                                       "\\f",
                                       "\\n",
                                       "\\r",
                                       "\\t",
                                       "\\x",
                                       "\\u",
                                       "\\u00e9",
                                       "\\uD83D",
                                       "\\uDE00",
                                       "\\uDBFF\\uDFFF",
                                       "\\u12G4",
                                       "\xc3\xa9",
                                       "\xff",
                                       "\\u0000",
                                       "\\u0041",
                                       "\\u07FF",
                                       "\\u0800",
                                       "\\uFFFF",
                                       "\\uDC00",
                                       "\\uD800\\n",
                                       "\xf0\x9f\x98\x80"};
  for (uint32_t i = 0; i < len; ++i) {
    if (rng.below(8) == 0) {
      out += pieces[rng.below(sizeof(pieces) / sizeof(pieces[0]))];
    } else {
      out += static_cast<char>('a' + rng.below(26));
    }
  }
  if (rng.below(40) != 0) out += '"';
}

void appendValue(Rng& rng, std::string& out, int depth) {
  switch (rng.below(depth > 36 ? 6 : 9)) {
    case 0:
      appendString(rng, out);
      break;
    case 1: {
      static const char* const numbers[] = {"0", "-1", "12.5", "1e9", "-2.5E-3", "+4", "1.2.3", "--", "7e"};
      out += numbers[rng.below(sizeof(numbers) / sizeof(numbers[0]))];
      break;
    }
    case 2:
    case 3: {
      static const char* const literals[] = {"true", "false", "null", "tru", "nul", "fals e", "nulll"};
      out += literals[rng.below(sizeof(literals) / sizeof(literals[0]))];
      break;
    }
    case 4:
      out += static_cast<char>(rng.below(256));
      break;
    case 5:
      out += " \t\n";
      break;
    case 6:
    case 7: {
      out += '{';
      const uint32_t n = rng.below(4);
      for (uint32_t i = 0; i < n; ++i) {
        if (i) out += ',';
        appendString(rng, out);
        out += rng.below(10) ? ":" : "";
        appendValue(rng, out, depth + 1);
      }
      if (rng.below(20)) out += '}';
      break;
    }
    default: {
      out += '[';
      const uint32_t n = rng.below(4);
      for (uint32_t i = 0; i < n; ++i) {
        if (i) out += ',';
        appendValue(rng, out, depth + 1);
      }
      if (rng.below(20)) out += ']';
      break;
    }
  }
}

void mutate(Rng& rng, std::string& doc) {
  const uint32_t edits = rng.below(4);
  for (uint32_t i = 0; i < edits && !doc.empty(); ++i) {
    const size_t at = rng.below(static_cast<uint32_t>(doc.size()));
    switch (rng.below(3)) {
      case 0:
        doc[at] = static_cast<char>(rng.below(256));
        break;
      case 1:
        doc.erase(at, 1);
        break;
      default:
        doc.insert(doc.begin() + static_cast<std::ptrdiff_t>(at), "\"\\{}[]:,tfn-1u"[rng.below(14)]);
        break;
    }
  }
}

// Feeds both parsers the same pieces, empty ones included; their error state must agree after
// every piece.
void compare(const std::string& doc, Rng& rng, int round, uint32_t mask = ~0u) {
  Log mine, theirs;
  StreamingJsonParser ours(callbacks<JsonCallbacks>(&mine, mask));
  ours.setTruncateLongStrings(false);  // the SDK drops a long string value
  SdkStreamingJsonParser sdk(callbacks<SdkJsonCallbacks>(&theirs, mask));
  for (int pass = 0; pass < 2; ++pass) {
    size_t pos = 0;
    while (pos < doc.size()) {
      if (rng.below(16) == 0) {
        ours.feed(doc.data() + pos, 0);
        sdk.feed(doc.data() + pos, 0);
        ours.feed(nullptr, 0);
        sdk.feed(nullptr, 0);
      }
      const size_t n = std::min<size_t>(doc.size() - pos, 1 + rng.below(64));
      ours.feed(doc.data() + pos, n);
      sdk.feed(doc.data() + pos, n);
      pos += n;
      ASSERT_EQ(ours.hasError(), sdk.hasError()) << "round " << round << " at " << pos;
    }
    ASSERT_EQ(mine.text, theirs.text) << "round " << round << " pass " << pass;
    // The second pass reads the document again after a reset.
    ours.reset();
    sdk.reset();
    mine.text.clear();
    theirs.text.clear();
  }
}

TEST(JsonSaxOracle, GeneratedDocumentsMatchTheSdkParser) {
  Rng rng{0x2545F491u};
  for (int round = 0; round < 20000; ++round) {
    std::string doc;
    appendValue(rng, doc, 0);
    if (rng.below(2)) mutate(rng, doc);
    compare(doc, rng, round, rng.below(8) == 0 ? rng.next() : ~0u);
    if (HasFatalFailure()) return;
  }
}

TEST(JsonSaxOracle, NestingAtTheLimitMatches) {
  Rng rng{7};
  for (const size_t depth : {31u, 32u, 33u, 40u}) {
    compare(std::string(depth, '[') + "1" + std::string(depth, ']'), rng, static_cast<int>(depth));
    compare(std::string(depth, '{') + std::string(depth, '}'), rng, static_cast<int>(depth));
  }
}

TEST(JsonSaxOracle, TokensAtTheBufferEdgeMatch) {
  Rng rng{11};
  for (size_t len = 2040; len <= 2056; ++len) {
    compare("[\"" + std::string(len, 'x') + "\", \"next\", " + std::string(len, '9') + ", 1]", rng,
            static_cast<int>(len));
    compare("{\"" + std::string(len, 'k') + "\": 1, \"b\": \"" + std::string(len - 2, 'y') + "\\u00e9\"}", rng,
            static_cast<int>(len));
    // A 4-byte character from an escape pair, and one written as UTF-8, across the edge.
    compare(
        "[\"" + std::string(len - 3, 'z') + "\\ud83d\\ude00\", \"" + std::string(len - 3, 'w') + "\xf0\x9f\x98\x80\"]",
        rng, static_cast<int>(len));
  }
}

}  // namespace
