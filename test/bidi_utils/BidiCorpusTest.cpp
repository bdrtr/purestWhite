// BidiUtils over a generated corpus: 32 batches of 1000 pseudo-random lines drawn from every bidi
// class the table knows (letters of each script, digits, separators, marks, formatting controls,
// brackets, joiners), at auto, LTR and RTL paragraph levels, up to past the 128-character limit.
// Each batch's answers hash to a recorded value: first the C implementation's (lib/MiniBidi),
// since the UAX #9 conformance work the Rust one's. A changed hash names the batch; rerun with
// BIDI_CORPUS_PRINT=<batch> to list its lines.

#include <gtest/gtest.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include "BidiUtils.h"
#include "Utf8.h"

namespace {

constexpr uint32_t kAlphabet[] = {
    'a',    'Z',     0x00E9, 0x0436,                  // L: Latin, Latin-1, Cyrillic
    0x05D0, 0x05D4,  0x05D5, 0x05D9, 0x05E9, 0x05DD,  // R: Hebrew, with the Tetragrammaton's letters
    0x0627, 0x0644,  0x0622, 0x0628, 0x062A, 0x0647,  // AL: Arabic, with Lam-Alef pairs
    0x067E, 0x06AF,  0x06CC, 0x06D2, 0x06BA, 0x0640,  // AL: Perso-Arabic, Tatweel
    0x0621, 0x0629,  0x0649, 0xFEFB,                  // AL: non-joining, right-joining, a ligature
    '0',    '7',     0x06F3,                          // EN
    0x0661, 0x0669,  0x066B,                          // AN
    '+',    '-',                                      // ES
    '$',    '%',     '#',                             // ET
    ',',    '.',     ':',    '/',    0x00A0,          // CS
    0x064E, 0x0651,  0x05B0, 0x05B8, 0x05C1, 0x0301,  // NSM
    0x00AD, 0x200B,  0xFEFF,                          // BN
    0x200C, 0x200D,                                   // ZWNJ, ZWJ
    '\t',   0x2029,  ' ',    ' ',    ' ',    0x2028,  // S, B, WS
    '!',    '"',     '(',    ')',    '[',    ']',    '<', '>', '{', '}',  // ON and brackets
    0x00AB, 0x00BB,  0x2018, 0x2019, 0x201C, 0x2039,                      // quotes and brackets the table pairs
    0x202A, 0x202B,  0x202C, 0x202D, 0x202E,                              // LRE RLE PDF LRO RLO
    0x2066, 0x2067,  0x2068, 0x2069,                                      // LRI RLI FSI PDI
    0x200E, 0x200F,                                                       // LRM RLM
    0x4E2D, 0x1F600,                                                      // not in the table: ON
};
constexpr size_t kBatches = 32;
constexpr size_t kLinesPerBatch = 1000;

// Recorded with the UAX #9 resolution (100% of BidiTest.txt and BidiCharacterTest.txt) and L4
// mirroring by each character's own level.
constexpr uint64_t kBatchHashes[kBatches] = {
    0x7ED15D40E2346481ULL, 0x85030937D27E8293ULL, 0x3CCF1D152FE15F42ULL, 0xDEC70454D05976D4ULL, 0x977A0483EC2F6990ULL,
    0x202BA7084E82C671ULL, 0xDB149447EED7FA22ULL, 0x9BADB94E6BE85ED9ULL, 0xE5816E5A56679829ULL, 0x6DAC547A317E75C6ULL,
    0xB1CD5F9992B65166ULL, 0xD63908734310596CULL, 0x28353D4E9005AADBULL, 0xD17B74B68145543AULL, 0x05EE45CC27E7DE5DULL,
    0x8B2E67B5B93A0EAEULL, 0x733503A5254022BAULL, 0x2BB6EDFD5C9D20F0ULL, 0xF9E8A8204E1C43A1ULL, 0x1674598A0F1784B2ULL,
    0x2AF7BB37B8DEFADCULL, 0x4DE8D9EA0716A950ULL, 0xAADC2B1B70711F20ULL, 0xCFB00EF043081A00ULL, 0xC5EE4A28F6AA78C4ULL,
    0xB96A79066C974237ULL, 0x12E6F405382053F7ULL, 0x09E2902C2333382FULL, 0xEBEC309B1C88A3D8ULL, 0xB1D7907C72E306C1ULL,
    0x3B590F3DB21DBFB4ULL, 0x935DC2AE08AAB898ULL};

struct Rng {
  uint64_t state;
  uint32_t next() {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    return static_cast<uint32_t>(state >> 32);
  }
  uint32_t below(const uint32_t n) { return next() % n; }
};

struct Hash {
  uint64_t value = 0xCBF29CE484222325ULL;  // FNV-1a
  void add(const void* data, const size_t len) {
    for (size_t i = 0; i < len; ++i) {
      value ^= static_cast<const uint8_t*>(data)[i];
      value *= 0x100000001B3ULL;
    }
  }
  void add(const std::string& s) { add(s.data(), s.size() + 1); }
  void add(const uint8_t byte) { add(&byte, 1); }
};

std::vector<uint32_t> line(Rng& rng) {
  // Mostly short lines, some past the 128-character limit.
  const uint32_t length = rng.below(8) == 0 ? 100 + rng.below(40) : 1 + rng.below(24);
  std::vector<uint32_t> cps(length);
  for (uint32_t& cp : cps) cp = kAlphabet[rng.below(sizeof(kAlphabet) / sizeof(kAlphabet[0]))];
  return cps;
}

std::string hexLine(const std::string& utf8) {
  std::string out;
  auto* p = reinterpret_cast<const unsigned char*>(utf8.c_str());
  while (*p) {
    char b[12];
    snprintf(b, sizeof b, "%04X ", utf8NextCodepoint(&p));
    out += b;
  }
  return out;
}

uint64_t runBatch(const size_t batch, const bool print) {
  Rng rng{0x9E3779B97F4A7C15ULL * (batch + 1)};
  Hash hash;
  for (size_t n = 0; n < kLinesPerBatch; ++n) {
    const std::vector<uint32_t> cps = line(rng);
    std::string text;
    for (const uint32_t cp : cps) utf8AppendCodepoint(cp, text);
    const int level = static_cast<int>(rng.below(3)) - 1;

    std::string visual;
    const bool ok = BidiUtils::applyBidiVisual(text.c_str(), visual, level);
    hash.add(static_cast<uint8_t>(ok));
    hash.add(ok ? visual : std::string());

    // The same characters as words split at the spaces.
    std::vector<std::string> words(1);
    for (const uint32_t cp : cps) {
      if (cp == ' ') {
        words.emplace_back();
      } else {
        utf8AppendCodepoint(cp, words.back());
      }
    }
    const bool rtl = rng.below(2) == 1;
    std::vector<uint16_t> order;
    const bool reordered = BidiUtils::computeVisualWordOrder(words, rtl, order);
    hash.add(static_cast<uint8_t>(reordered));
    hash.add(order.data(), order.size() * sizeof(order[0]));

    hash.add(static_cast<uint8_t>(BidiUtils::startsWithRtl(text.c_str())));
    hash.add(static_cast<uint8_t>(BidiUtils::detectParagraphLevel(text.c_str(), level & 1)));

    if (print) {
      std::string orderText;
      for (const uint16_t w : order) orderText += std::to_string(w) + " ";
      printf("%zu level %d: %s\n  -> %d %s\n  words rtl=%d -> %d %s\n", n, level, hexLine(text).c_str(), ok,
             ok ? hexLine(visual).c_str() : "", rtl, reordered, orderText.c_str());
    }
  }
  return hash.value;
}

}  // namespace

TEST(BidiCorpus, EveryBatchHashesAsBefore) {
  const char* printBatch = std::getenv("BIDI_CORPUS_PRINT");
  for (size_t batch = 0; batch < kBatches; ++batch) {
    const bool print = printBatch && std::strtoul(printBatch, nullptr, 10) == batch;
    const uint64_t hash = runBatch(batch, print);
    EXPECT_EQ(hash, kBatchHashes[batch]) << "batch " << batch << ": 0x" << std::hex << hash;
  }
}
