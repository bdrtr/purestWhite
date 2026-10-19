#include "Hyphenator.h"

#include <bembeyaz/hyphenation.h>

int Hyphenator::language_ = -1;

std::vector<Hyphenator::BreakInfo> Hyphenator::breakOffsets(const std::string& word, const bool includeFallback) {
  std::vector<BreakInfo> breaks;
  bb_hyph_breaks(
      language_, word.data(), word.size(), includeFallback,
      [](void* context, const size_t offset, const bool insertedHyphen) noexcept {
        static_cast<std::vector<BreakInfo>*>(context)->push_back({offset, insertedHyphen});
      },
      &breaks);
  return breaks;
}

void Hyphenator::setPreferredLanguage(const std::string& lang) {
  language_ = bb_hyph_language(lang.data(), lang.size());
}
