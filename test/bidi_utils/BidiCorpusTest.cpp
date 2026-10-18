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

// Recorded with the UAX #9 resolution (100% of BidiTest.txt and BidiCharacterTest.txt).
constexpr uint64_t kBatchHashes[kBatches] = {
    0xAB7E09D4BF884D56ULL, 0xE0FB9B913A79499FULL, 0xF26CF653C34EDC5EULL, 0x475139F77EF9AA81ULL, 0xC6996713E8D1911FULL,
    0xAFACFB31C53ABD9AULL, 0x83EB7A96D45D7FC4ULL, 0x110F7EEFE9AFEF3CULL, 0x1BF37E434CF4B78DULL, 0xBE5DBDFD03BBE919ULL,
    0xA5019E4D56372370ULL, 0x8F3B29325F939250ULL, 0x73BA1542B655DA13ULL, 0xBBFBB5AAFF8B3038ULL, 0x6E6BD8208FD2915FULL,
    0xA9C1BECB774FE9E3ULL, 0x36F4CCEF33BC3415ULL, 0x5CDA0E61FE4038DEULL, 0xEE9B952B9542F215ULL, 0x042FC44A88F6A245ULL,
    0xD98A9943B3BD4DFEULL, 0xDB6FBD9646253A76ULL, 0xC100EA56EC7FA171ULL, 0x44FF0BF79B255F1AULL, 0x5AFF5D01D881E36FULL,
    0xA317A2F35F30A156ULL, 0x010C7DB3F5B72DF0ULL, 0xF711E06C3250CE31ULL, 0x242BC56B233B20B8ULL, 0x588D1DAD45F62D56ULL,
    0xA77EB019EDAF1ED9ULL, 0xEBA7A7FA50132DD5ULL};

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
