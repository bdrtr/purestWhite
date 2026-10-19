// Hyphenator::breakOffsets over every word of the evaluation lists and over generated words:
// capitals, words joined by hyphens, dashes and apostrophes, surrounding punctuation, footnote
// marks, decomposed accents, digits, CJK and Hangul, over-long words, malformed UTF-8 and NULs,
// each with and without fallback breaks, under every language and some unknown ones. The
// answers of each language hash to a value recorded from the C++ implementation. A changed
// hash names the language; rerun with HYPHENATION_CORPUS_PRINT=<tag> to list its cases. To
// re-record (after new tries), set every kHashes entry to 0 and paste the printed values.

#include <Utf8.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

#include "lib/Epub/Epub/hyphenation/Hyphenator.h"

#ifndef HYPHENATION_RESOURCES_DIR
#error "HYPHENATION_RESOURCES_DIR must be defined by the build system"
#endif

namespace {

struct Rng {
  uint64_t state;
  uint32_t next() {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    return static_cast<uint32_t>(state >> 32);
  }
  uint32_t below(const uint32_t n) { return next() % n; }
  bool chance(const uint32_t n) { return below(n) == 0; }
  template <typename T>
  const T& pick(const std::vector<T>& items) {
    return items[below(static_cast<uint32_t>(items.size()))];
  }
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
};

struct Language {
  const char* tag;       // passed to setPreferredLanguage
  const char* resource;  // evaluation list, or nullptr
  // Letters for generated words; uppercase letters are made from these.
  std::vector<uint32_t> letters;
};

std::vector<uint32_t> range(const uint32_t first, const uint32_t last, std::vector<uint32_t> extra = {}) {
  for (uint32_t cp = first; cp <= last; ++cp) extra.push_back(cp);
  return extra;
}

const std::vector<uint32_t> kLatin = range('a', 'z');
const std::vector<uint32_t> kCyrillic = range(0x430, 0x44F, {0x451});

std::vector<Language> languages() {
  return {
      {"en", "english_hyphenation_tests.txt", kLatin},
      {"fr", "french_hyphenation_tests.txt",
       range('a', 'z', {0xE0, 0xE2, 0xE7, 0xE8, 0xE9, 0xEA, 0xEB, 0xEE, 0xF4, 0xF9, 0xFB, 0x153})},
      {"de", "german_hyphenation_tests.txt", range('a', 'z', {0xE4, 0xF6, 0xFC, 0xDF})},
      {"ru", "russian_hyphenation_tests.txt", kCyrillic},
      {"es", "spanish_hyphenation_tests.txt", range('a', 'z', {0xE1, 0xE9, 0xED, 0xF1, 0xF3, 0xFA, 0xFC})},
      {"it", "italian_hyphenation_tests.txt", range('a', 'z', {0xE0, 0xE8, 0xE9, 0xEC, 0xF2, 0xF9})},
      {"pl", "polish_hyphenation_tests.txt",
       range('a', 'z', {0x105, 0x107, 0x119, 0x142, 0x144, 0xF3, 0x15B, 0x17A, 0x17C})},
      {"sv", "swedish_hyphenation_tests.txt", range('a', 'z', {0xE4, 0xE5, 0xF6})},
      {"uk", nullptr, range(0x430, 0x44F, {0x454, 0x456, 0x457, 0x491})},
      {"fi", nullptr, range('a', 'z', {0xE4, 0xF6})},
      {"pt", "portuguese_hyphenation_tests.txt",
       range('a', 'z', {0xE0, 0xE1, 0xE2, 0xE3, 0xE7, 0xE9, 0xEA, 0xED, 0xF3, 0xF4, 0xF5, 0xFA})},
      // No patterns: only explicit, apostrophe and fallback breaks.
      {"ko", nullptr, range(0xAC00, 0xAC40)},
      {"tr", nullptr, range('a', 'z', {0xE7, 0x11F, 0x131, 0xF6, 0x15F, 0xFC})},
  };
}

uint32_t upper(const uint32_t cp) {
  if (cp >= 'a' && cp <= 'z') return cp - 0x20;
  if (cp >= 0xE0 && cp <= 0xFE && cp != 0xF7) return cp - 0x20;
  if (cp >= 0x100 && cp <= 0x17F) return cp & ~1u;  // close enough for generated words
  if (cp >= 0x430 && cp <= 0x44F) return cp - 0x20;
  if (cp >= 0x450 && cp <= 0x45F) return cp - 0x50;
  if (cp == 0x491) return 0x490;
  return cp;
}

// Precomposed letters and the base + combining mark they decompose into.
struct Decomposition {
  uint32_t composed, base, mark;
};
const Decomposition kDecompositions[] = {
    {0xE9, 'e', 0x301},  {0xC9, 'E', 0x301},  {0xE8, 'e', 0x300},  {0xEA, 'e', 0x302},  {0xF1, 'n', 0x303},
    {0xFC, 'u', 0x308},  {0xD6, 'O', 0x308},  {0xE4, 'a', 0x308},  {0xE7, 'c', 0x327},  {0x17C, 'z', 0x307},
    {0x105, 'a', 0x328}, {0x119, 'e', 0x328}, {0x144, 'n', 0x301}, {0x15B, 's', 0x301}, {0xFF, 'y', 0x308},
    {0x178, 'Y', 0x308}, {0xE3, 'a', 0x303},  {0xF4, 'o', 0x302},  {0xE0, 'a', 0x300},  {0x107, 'c', 0x301},
};

// Every base the C++ code composes with each mark, and some it does not.
struct MarkBases {
  uint32_t mark;
  const char* bases;
};
const MarkBases kMarkBases[] = {
    {0x300, "AaEeIiOoUuYb"},
    {0x301, "AaEeIiOoUuYyCcNnSsZzGk"},
    {0x302, "AaEeIiOoUuCx"},
    {0x303, "AaNnOo"},
    {0x308, "AaEeIiOoUuYyTx"},
    {0x307, "ZzCe"},
    {0x327, "CcSs"},
    {0x328, "AaEeIo"},
    {0x30A, "Aa"},
};

std::string encode(const std::vector<uint32_t>& cps) {
  std::string out;
  for (const uint32_t cp : cps) utf8AppendCodepoint(cp, out);
  return out;
}

std::vector<uint32_t> decode(const std::string& s) {
  std::vector<uint32_t> cps;
  const auto* p = reinterpret_cast<const unsigned char*>(s.c_str());
  while (*p) cps.push_back(utf8NextCodepoint(&p));
  return cps;
}

std::vector<std::string> loadWords(const char* resource) {
  std::vector<std::string> words;
  if (!resource) return words;
  std::ifstream file(std::string(HYPHENATION_RESOURCES_DIR) + "/" + resource);
  std::string line;
  while (std::getline(file, line)) {
    if (line.empty() || line[0] == '#') continue;
    words.push_back(line.substr(0, line.find('|')));
  }
  return words;
}

std::vector<uint32_t> randomWord(Rng& rng, const Language& lang, const std::vector<std::string>& words) {
  if (!words.empty() && !rng.chance(4)) return decode(rng.pick(words));
  std::vector<uint32_t> cps;
  const uint32_t len = 1 + rng.below(rng.chance(8) ? 40 : 12);
  for (uint32_t i = 0; i < len; ++i) cps.push_back(rng.pick(lang.letters));
  return cps;
}

const std::vector<uint32_t> kJoiners = {'-',    0xAD, 0x2010, 0x2011, 0x2013, 0x2014, 0x2212, '_',
                                        0x2026, '\'', 0x2018, 0x2019, 0xFF0D, 0x58A,  0x2E3A, 0x207B};
const std::vector<uint32_t> kPunctuation = {
    '.',    ',',    '!',  '?', ';', ':', '"', '\'', '(',    ')',    0xAB,   0xBB, 0x2018, 0x2019, 0x201A, 0x201C,
    0x201D, 0x201E, 0xA0, '{', '}', '[', ']', '/',  0x2039, 0x203A, 0x2026, '-',  '*',    0x2022, 0xBF,   0xA1};
const std::vector<uint32_t> kOdd = {'0',     '7',     '9',     0x3042,  0x4E2D, 0x30AB, 0xAC00, 0xD55C, 0xFF21, 0x3001,
                                    0x1100,  0x5D0,   0x627,   0x3B1,   0x391,  0x1E9E, 0xDF,   0x152,  0x153,  0x178,
                                    0x130,   0x131,   0x400,   0x401,   0x40F,  0x404,  0x490,  0x52F,  0x530,  0xD7,
                                    0xF7,    0x17F,   0x180,   0x301,   0x308,  0x30A,  0x200B, 0x200D, 0xFEFF, 0xFFFD,
                                    0x1F600, 0x10000, 0x20000, 0x2A700, ' ',    '\t',   '@',    '#',    '&',    '+'};

std::string generate(Rng& rng, const Language& lang, const std::vector<std::string>& words) {
  std::vector<uint32_t> cps = randomWord(rng, lang, words);

  // Capitals.
  if (rng.chance(4) && !cps.empty()) cps[0] = upper(cps[0]);
  if (rng.chance(10))
    for (auto& cp : cps) cp = upper(cp);

  // Compounds.
  const uint32_t parts = rng.chance(3) ? 1 + rng.below(3) : 0;
  for (uint32_t i = 0; i < parts; ++i) {
    if (!rng.chance(5)) cps.push_back(rng.pick(kJoiners));
    if (rng.chance(8)) cps.push_back(rng.pick(kJoiners));
    const auto more = rng.chance(3) ? std::vector<uint32_t>{rng.pick(lang.letters), rng.pick(lang.letters)}
                                    : randomWord(rng, lang, words);
    cps.insert(cps.end(), more.begin(), more.end());
  }

  // Odd characters inside.
  if (rng.chance(6)) {
    const uint32_t n = 1 + rng.below(3);
    for (uint32_t i = 0; i < n; ++i) cps.insert(cps.begin() + rng.below(cps.size() + 1), rng.pick(kOdd));
  }

  // Decomposed accents, some of which compose back.
  if (rng.chance(5)) {
    std::vector<uint32_t> out;
    for (const uint32_t cp : cps) {
      bool done = false;
      for (const auto& d : kDecompositions) {
        if (d.composed == cp && rng.chance(2)) {
          out.push_back(d.base);
          out.push_back(d.mark);
          done = true;
          break;
        }
      }
      if (!done) out.push_back(cp);
      if (rng.chance(30)) out.push_back(rng.pick(std::vector<uint32_t>{0x301, 0x308, 0x30A, 0x327}));
    }
    cps = out;
  }
  if (rng.chance(4)) {
    const uint32_t n = 1 + rng.below(3);
    for (uint32_t i = 0; i < n; ++i) {
      const MarkBases& m = kMarkBases[rng.below(sizeof kMarkBases / sizeof kMarkBases[0])];
      const uint32_t at = rng.below(cps.size() + 1);
      cps.insert(cps.begin() + at, {static_cast<uint32_t>(m.bases[rng.below(strlen(m.bases))]), m.mark});
    }
  }

  // Over-long words: past the 68 letters and 160 bytes of the pattern buffers, past 256.
  if (rng.chance(25)) {
    const std::vector<uint32_t> copy = cps;
    const uint32_t times = 2 + rng.below(rng.chance(3) ? 40 : 8);
    for (uint32_t i = 0; i < times && !copy.empty(); ++i) cps.insert(cps.end(), copy.begin(), copy.end());
  }

  // Surrounding punctuation and footnote marks.
  if (rng.chance(3)) {
    const uint32_t n = 1 + rng.below(3);
    for (uint32_t i = 0; i < n; ++i) cps.insert(cps.begin(), rng.pick(kPunctuation));
  }
  if (rng.chance(5)) {
    cps.push_back('[');
    const uint32_t digits = rng.below(4);
    for (uint32_t i = 0; i < digits; ++i) cps.push_back('0' + rng.below(10));
    if (rng.chance(6)) cps.push_back('a');
    if (!rng.chance(6)) cps.push_back(']');
  }
  if (rng.chance(3)) {
    const uint32_t n = 1 + rng.below(3);
    for (uint32_t i = 0; i < n; ++i) cps.push_back(rng.pick(kPunctuation));
  }

  std::string word = encode(cps);
  // Malformed UTF-8 and embedded NULs.
  if (rng.chance(30) && !word.empty()) {
    const char bad[] = {'\x80', '\xC3', '\xE2', '\xF0', '\xFF', '\xC0', '\xED', '\0'};
    word.insert(word.begin() + rng.below(word.size() + 1), bad[rng.below(sizeof bad)]);
  }
  if (rng.chance(200)) word.clear();
  return word;
}

std::string answer(const std::string& word, const bool fallback) {
  std::string out;
  for (const auto& b : Hyphenator::breakOffsets(word, fallback)) {
    out += std::to_string(b.byteOffset);
    out += b.requiresInsertedHyphen ? "+ " : " ";
  }
  return out;
}

std::string hex(const std::string& s) {
  std::string out;
  char h[3];
  for (const unsigned char c : s) {
    snprintf(h, sizeof h, "%02x", c);
    out += h;
  }
  return out;
}

uint64_t runLanguage(const Language& lang, const size_t index) {
  const char* printTag = std::getenv("HYPHENATION_CORPUS_PRINT");
  const bool print = printTag && std::string(printTag) == lang.tag;
  Hash hash;
  const auto note = [&](const std::string& word, const std::string& got) {
    hash.add(word);
    hash.add(got);
    if (print) printf("%s [%s] => %s\n", word.c_str(), hex(word).c_str(), got.c_str());
  };

  Hyphenator::setPreferredLanguage(lang.tag);
  const std::vector<std::string> words = loadWords(lang.resource);
  for (size_t i = 0; i < words.size(); ++i) {
    note(words[i], answer(words[i], (i & 1) != 0));
  }
  Rng rng{0x9E3779B97F4A7C15ULL * (index + 1)};
  for (int i = 0; i < 6000; ++i) {
    const std::string word = generate(rng, lang, words);
    note(word, answer(word, false) + "| " + answer(word, true));
  }
  return hash.value;
}

// Recorded from the C++ implementation (lib/Epub/Epub/hyphenation at 46ebf6e), in the order of
// languages().
constexpr uint64_t kHashes[] = {
    0x875D60C2E5CFD3D8ULL, 0x21C572224F67E823ULL, 0x4A615F7D5168561BULL, 0x89F259160EB845A3ULL, 0x1B824756C33FC4BFULL,
    0xA4A4B5DDED07EECBULL, 0x90A8E17191C8FB8EULL, 0x54778D33630FF205ULL, 0x636D9CAB32F3E4F1ULL, 0x4A0E8D8F5CB8E1A6ULL,
    0x802FC4426275DDC7ULL, 0x65AD195B30C137EBULL, 0x48CAEB1AA7870496ULL};

TEST(HyphenatorCorpusTest, MatchesRecordedAnswers) {
  const auto langs = languages();
  ASSERT_EQ(langs.size(), sizeof kHashes / sizeof kHashes[0]);
  bool record = true;
  for (const uint64_t h : kHashes) record = record && h == 0;
  for (size_t i = 0; i < langs.size(); ++i) {
    const uint64_t got = runLanguage(langs[i], i);
    if (record) {
      printf("0x%016llXULL,%s", static_cast<unsigned long long>(got), i % 4 == 3 ? "\n" : " ");
    } else {
      EXPECT_EQ(got, kHashes[i]) << "language " << langs[i].tag;
    }
  }
  if (record) printf("\n");
  EXPECT_FALSE(record) << "no hashes recorded yet";
}

// Language tags: primary subtag, case, ISO 639-2 codes, and tags with no patterns.
TEST(HyphenatorCorpusTest, ChoosesPatternsByLanguageTag) {
  const std::string word = "hyphenation";
  const std::string german = "Quadratkilometer";
  std::string got;
  for (const char* tag :
       {"en",  "EN-us", "eng", "En_GB", "fre", "fra",  "deu",     "ger",        "de_AT", "DE",  "ru",  "rus", "es",
        "spa", "it",    "ita", "pl",    "pol", "sv",   "swe",     "uk",         "ukr",   "fi",  "fin", "pt",  "pt-BR",
        "por", "ko",    "",    "-en",   "e",   "eng-", "english", "en\xC3\xA9", "tr",    "tur", "ENG"}) {
    Hyphenator::setPreferredLanguage(tag);
    got += std::string(tag) + ": " + answer(word, false) + "/ " + answer(german, false) + "\n";
  }
  Hyphenator::setPreferredLanguage(std::string("en\0x", 4));
  got += "en NUL: " + answer(word, false) + "\n";
  Hash hash;
  hash.add(got);
  if (std::getenv("HYPHENATION_CORPUS_PRINT")) printf("%s", got.c_str());
  EXPECT_EQ(hash.value, 0x650DE8754E25C4C5ULL) << got;
}

}  // namespace
