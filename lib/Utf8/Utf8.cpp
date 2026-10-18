#include "Utf8.h"

#include <bembeyaz/utf8.h>

// Implemented in Rust (rust/crates/bembeyaz-utf8); this file adapts std::string.

uint32_t utf8DecomposedBase(const uint32_t cp) { return bb_utf8_decomposed_base(cp); }

std::string utf8ComposeNfc(const std::string& in, bool (*onAbsorbed)(uint32_t, void*), void* context) {
  std::string out;
  const auto append = [](const char* data, const size_t len, void* target) {
    static_cast<std::string*>(target)->append(data, len);
  };
  switch (bb_utf8_compose_nfc(in.data(), in.size(), onAbsorbed, context, append, &out)) {
    case BB_UTF8_COMPOSE_UNCHANGED:
      return in;
    case BB_UTF8_COMPOSE_DONE:
      return out;
    default:
      return {};
  }
}

void utf8ComposeNfcInPlace(char* buffer) { bb_utf8_compose_nfc_in_place(buffer); }

uint32_t utf8NextCodepoint(const unsigned char** string) { return bb_utf8_next_codepoint(string); }

void utf8AppendCodepoint(uint32_t cp, std::string& out) {
  char bytes[4];
  const char* end = utf8AppendCodepoint(bytes, cp);
  out.append(bytes, static_cast<size_t>(end - bytes));
}

int utf8SafeTruncateBuffer(const char* buf, int len) {
  if (len <= 0) return 0;
  return static_cast<int>(bb_utf8_safe_truncate_len(buf, static_cast<size_t>(len)));
}

size_t utf8RemoveLastChar(std::string& str) {
  str.resize(bb_utf8_truncate_chars_len(str.data(), str.size(), 1));
  return str.size();
}

void utf8TruncateChars(std::string& str, const size_t numChars) {
  str.resize(bb_utf8_truncate_chars_len(str.data(), str.size(), numChars));
}
