#pragma once

#include <cstddef>
#include <string>
#include <vector>

// A shim over the Rust bembeyaz-hyphenation crate (rust/include/bembeyaz/hyphenation.h).
class Hyphenator {
 public:
  struct BreakInfo {
    size_t byteOffset;            // Byte position inside the UTF-8 word where a break may occur.
    bool requiresInsertedHyphen;  // true = a visible '-' must be rendered at the break (pattern/fallback breaks).
                                  // false = break occurs at an existing visible separator boundary
                                  //         (explicit '-' or eligible apostrophe contraction boundary),
                                  //         or next to a CJK character.
  };

  // Returns byte offsets where the word may be hyphenated.
  //
  // Break sources (in priority order):
  //   1. Explicit hyphens already present in the word (e.g. '-' or soft-hyphen U+00AD).
  //      When found, language patterns are additionally run on each alphabetic segment
  //      between separators so compound words can break within their parts.
  //      Example: "US-Satellitensystems" yields breaks after "US-" (no inserted hyphen)
  //               plus pattern breaks inside "Satellitensystems" (Sa|tel|li|ten|sys|tems).
  //   2. Apostrophe contractions between letters (e.g. all'improvviso).
  //      Liang patterns are run per alphabetic segment around apostrophes.
  //      A direct break at the apostrophe boundary is allowed only when the left
  //      segment has at least 3 letters and the right segment has at least 3 letters,
  //      avoiding short clitics (e.g. l', d') and contraction tails (e.g. 've, 're, 'll).
  //   3. Language-specific Liang patterns (typst/hypher tries in rust/crates/bembeyaz-hyphenation/tries).
  //      Example: "Quadratkilometer" -> Qua|drat|ki|lo|me|ter.
  //   4. Fallback breaks before every character the minimum prefix/suffix allow (only when
  //      includeFallback is true AND no pattern breaks were found; per segment around
  //      apostrophes; never for words with explicit hyphens). Used as a last resort to prevent
  //      a single oversized word from overflowing the page width.
  static std::vector<BreakInfo> breakOffsets(const std::string& word, bool includeFallback);

  // Provide a publication-level language hint (e.g. "en", "en-US", "ru") used to select hyphenation rules.
  static void setPreferredLanguage(const std::string& lang);

 private:
  static int language_;  // from bb_hyph_language; -1 for none
};