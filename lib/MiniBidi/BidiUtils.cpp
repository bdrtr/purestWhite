#include "BidiUtils.h"

#include <Logging.h>
#include <bembeyaz/bidi.h>

#include <cstring>
#include <mutex>

// The algorithm is Rust (rust/crates/bembeyaz-bidi). It works in one line's workspace, shared
// by applyBidiVisual() and computeVisualWordOrder(); the mutex serialises them. Being static, the
// workspace starts zeroed, as the Rust side requires.
static std::mutex bidiMutex;
static bb_bidi_workspace workspace;

namespace BidiUtils {

bool startsWithRtl(const char* utf8, const int maxStrongChars) {
  if (!utf8 || maxStrongChars <= 0) return false;
  return bb_bidi_paragraph_level(utf8, std::strlen(utf8), 0, maxStrongChars) == 1;
}

int detectParagraphLevel(const char* utf8, const int fallbackLevel, const int maxStrongChars) {
  if (!utf8) return fallbackLevel & 1;
  return bb_bidi_paragraph_level(utf8, std::strlen(utf8), fallbackLevel, maxStrongChars);
}

bool isTransparentMark(const uint32_t cp) { return bb_bidi_is_transparent_mark(cp); }

bool applyBidiVisual(const char* utf8, std::string& out, const int paragraphLevel) {
  if (!utf8 || !*utf8) return false;
  char visual[BB_BIDI_OUT_SIZE];
  size_t visualLen = 0;
  const std::lock_guard<std::mutex> lock(bidiMutex);
  const int result = bb_bidi_visual(utf8, std::strlen(utf8), paragraphLevel, &workspace, visual, &visualLen);
  if (result == BB_BIDI_TOO_LONG) {
    LOG_DBG("BIDI", "applyBidiVisual: input exceeds %d chars, returning unprocessed", BB_BIDI_MAX_LINE);
  }
  if (result != BB_BIDI_DONE) return false;
  out.assign(visual, visualLen);
  return true;
}

bool computeVisualWordOrder(const std::vector<std::string>& words, const bool paragraphIsRtl,
                            std::vector<uint16_t>& visualOrder) {
  visualOrder.clear();
  const size_t count = words.size();
  if (count <= 1 || count > BB_BIDI_MAX_LINE) return false;
  // A word ends at its first NUL, as it did when the words were read as C strings.
  const auto word = [](const size_t index, size_t* len, const void* context) {
    const char* w = (*static_cast<const std::vector<std::string>*>(context))[index].c_str();
    *len = std::strlen(w);
    return w;
  };

  uint16_t order[BB_BIDI_MAX_LINE] = {};
  const std::lock_guard<std::mutex> lock(bidiMutex);
  if (!bb_bidi_word_order(count, word, &words, paragraphIsRtl, &workspace, order)) {
    return false;
  }
  visualOrder.assign(order, order + count);
  return true;
}

}  // namespace BidiUtils
