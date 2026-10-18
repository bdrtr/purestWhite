// BidiUtils over a generated corpus: 32 batches of 1000 pseudo-random lines drawn from every bidi
// class the table knows (letters of each script, digits, separators, marks, formatting controls,
// brackets, joiners), at auto, LTR and RTL paragraph levels, up to past the 128-character limit.
// Each batch's answers hash to the value the C implementation (lib/MiniBidi) gave. A changed hash
// names the batch; rerun with BIDI_CORPUS_PRINT=<batch> to list its lines.

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

// The values lib/MiniBidi gave.
constexpr uint64_t kBatchHashes[kBatches] = {
    0x65D853177CA77559ULL, 0x57019BE202B8A6CBULL, 0xF2FBBB74161F5F77ULL, 0x1B8822AF0A6AD22EULL, 0x27600DF08DBCE72DULL,
    0x0F8FE4EFAF87C589ULL, 0xDA876044ACA337E9ULL, 0x578C98A17BF7BABDULL, 0xC10BD1EC537B420EULL, 0x884830FB35702EE9ULL,
    0x7F6FECA730E70317ULL, 0x45604C251BE46A0FULL, 0x3F3A82419A875138ULL, 0xB0047692BAC3CB6BULL, 0xAF5AAECD9F17BFFFULL,
    0x4908F2E517150FDBULL, 0xA72705C423C87B36ULL, 0xE93DD4823F6FC116ULL, 0x761937AE6078F6BCULL, 0xB5208DF7F0E2A763ULL,
    0x9F53D4E24AB9CE20ULL, 0xB3A00DE104F5334BULL, 0xBC9A76B438361053ULL, 0xD53DDBFB37C2C70DULL, 0x71E1E8E7E0FF5CAEULL,
    0x368EF832A8869556ULL, 0xEB69A26FE2E08B5DULL, 0x4C8B1486A0510CEFULL, 0xF7E303CF52624C34ULL, 0xFBD33D4DC99BC853ULL,
    0x2667762BCFDE8A2FULL, 0xC4A9380A76490961ULL};

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
