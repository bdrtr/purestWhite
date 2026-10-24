// The dictionary reader (src/util/Dictionary.cpp, its index and lookups now bembeyaz-dict's)
// against the reader it had before (OldDictionary.cpp), over StarDict dictionaries in memory:
// random headwords, synonyms and .ifo files, damaged and truncated indexes and sidecars, failing
// reads and bad clusters, and random lookups must give the same sidecars, verdicts, definitions and headwords.
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <string>
#include <vector>

#include "OldDictionary.h"
#include "util/Dictionary.h"
#include "util/StringUtils.h"

namespace {

uint32_t seed;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}
int range(int lo, int hi) { return lo + static_cast<int>(rnd() % static_cast<uint32_t>(hi - lo + 1)); }

std::string randomWord(int maxLen) {
  static const char* const PIECES[] = {"a",
                                       "b",
                                       "c",
                                       "e",
                                       "i",
                                       "n",
                                       "s",
                                       "t",
                                       "y",
                                       "A",
                                       "S",
                                       "E",
                                       "1",
                                       "-",
                                       "'",
                                       "\xC3\xA9",
                                       "\xE2\x80\x99",
                                       "\xE2\x80\x9C",
                                       "d",
                                       "g"};
  std::string w;
  const int len = range(1, maxLen);
  while (static_cast<int>(w.size()) < len) w += PIECES[rnd() % 20];
  return w;
}

void put32(std::vector<uint8_t>& out, uint32_t v) {
  for (int s = 24; s >= 0; s -= 8) out.push_back(static_cast<uint8_t>(v >> s));
}

DictionaryTestFs copyOf(const DictionaryTestFs& fs) {
  DictionaryTestFs out;
  out.failReadsFrom = fs.failReadsFrom;
  out.badFrom = fs.badFrom;
  for (const auto& [path, data] : fs.files) out.files[path] = std::make_shared<std::vector<uint8_t>>(*data);
  return out;
}

bool sameFiles(const DictionaryTestFs& a, const DictionaryTestFs& b) {
  if (a.files.size() != b.files.size()) return false;
  for (const auto& [path, data] : a.files) {
    const auto it = b.files.find(path);
    if (it == b.files.end() || *it->second != *data) return false;
  }
  return true;
}

// A dictionary's files under /d/f/, and words worth looking up in it.
std::vector<std::string> makeDictionary() {
  dictionaryFs = {};
  auto& files = dictionaryFs.files;
  const auto file = [&](const char* suffix) {
    auto data = std::make_shared<std::vector<uint8_t>>();
    files[std::string("/d/f/dict") + suffix] = data;
    return data;
  };
  std::vector<std::string> words;
  const int count = rnd() % 4 == 0 ? range(0, 20) : range(200, 1500);
  for (int i = 0; i < count; ++i) words.push_back(randomWord(rnd() % 50 == 0 ? 300 : 10));
  std::sort(words.begin(), words.end(), [](const std::string& a, const std::string& b) {
    const int c = StringUtils::asciiCaseCmp(a.c_str(), b.c_str());
    return c != 0 ? c < 0 : a < b;
  });
  auto idx = file(".idx");
  auto dict = file(rnd() % 8 ? ".dict" : ".dict.dz");
  for (size_t i = 0; i < words.size(); ++i) {
    const std::string definition = "def " + std::to_string(i) + " " + words[i];
    idx->insert(idx->end(), words[i].begin(), words[i].end());
    idx->push_back(0);
    put32(*idx, rnd() % 50 ? static_cast<uint32_t>(dict->size()) : rnd());
    put32(*idx, rnd() % 50 ? static_cast<uint32_t>(definition.size()) : rnd() % 3);
    dict->insert(dict->end(), definition.begin(), definition.end());
  }
  static const char* const IFOS[] = {
      "StarDict's dict ifo file\nversion=2.4.2\nsametypesequence=m\n",
      "version=3.0.0\nidxoffsetbits=64\n",
      "idxoffsetbits=32\nsametypesequence=h\n",
      "sametypesequence=x\r\nidxoffsetbits= 64",
      "sametypesequence=hm\n",
      "",
  };
  const std::string ifo = IFOS[rnd() % 6];
  if (rnd() % 8) file(".ifo")->assign(ifo.begin(), ifo.end());
  std::vector<std::string> synonyms;
  if (rnd() % 2 && !words.empty()) {
    auto syn = file(".syn");
    for (int i = range(0, 600); i > 0; --i) synonyms.push_back(randomWord(9));
    std::sort(synonyms.begin(), synonyms.end(), [](const std::string& a, const std::string& b) {
      return StringUtils::asciiCaseCmp(a.c_str(), b.c_str()) < 0;
    });
    for (const auto& s : synonyms) {
      syn->insert(syn->end(), s.begin(), s.end());
      syn->push_back(0);
      put32(*syn, rnd() % 30 ? rnd() % words.size() : rnd() % (words.size() + 600));
    }
  }
  // Damage.
  if (rnd() % 6 == 0 && !idx->empty()) idx->resize(rnd() % idx->size());
  if (rnd() % 10 == 0) dictionaryFs.failReadsFrom["/d/f/dict.idx"] = rnd() % (idx->size() + 1);
  if (rnd() % 10 == 0) dictionaryFs.badFrom[rnd() % 2 ? "/d/f/dict.idx" : "/d/f/dict.syn"] = rnd() % (idx->size() + 1);
  std::vector<std::string> queries = words;
  queries.insert(queries.end(), synonyms.begin(), synonyms.end());
  return queries;
}

// A word to look up from the dictionary's words: as it is, in other case, with punctuation, with
// English endings, or made up.
std::string query(const std::vector<std::string>& words) {
  std::string w = words.empty() || rnd() % 6 == 0 ? randomWord(12) : words[rnd() % words.size()];
  if (rnd() % 3 == 0) {
    for (auto& c : w) {
      if (rnd() % 2) c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
    }
  }
  static const char* const ENDINGS[] = {"s", "es", "ies", "'s", "\xE2\x80\x99s", "ed", "ing", "", "", ""};
  if (rnd() % 2) w += ENDINGS[rnd() % 10];
  if (rnd() % 4 == 0) w = "\xE2\x80\x9C(" + w + ").\xE2\x80\x9D";
  if (rnd() % 40 == 0) w = std::string(range(250, 300), 'a');
  return w;
}

}  // namespace

TEST(Dictionary, MatchesTheOldReader) {
  seed = 47;
  unsigned diffs = 0;
  for (int c = 0; c < 1000; ++c) {
    const std::vector<std::string> words = makeDictionary();
    const DictionaryTestFs start = copyOf(dictionaryFs);
    // Sidecars damaged after a first build, or left stale, or built fresh.
    const int damage = rnd() % 4;
    const uint32_t damageSeed = rnd();
    std::vector<std::string> queries;
    for (int i = 0; i < 60; ++i) queries.push_back(query(words));

    struct Run {
      bool opened = false, html = false, neededIndex = false, built = false, neededAfter = false;
      int indexResult = -1;
      std::vector<std::string> results;
    };
    const auto run = [&](auto& d, auto indexResult, auto lookupResult) {
      Run r;
      uint32_t local = damageSeed;
      r.opened = d.open("f");
      r.html = d.definitionsAreHtml();
      r.neededIndex = d.needsIndex();
      decltype(indexResult) ir{};
      r.built = d.buildIndex(nullptr, nullptr, &ir);
      r.indexResult = static_cast<int>(ir);
      if (damage == 1 || damage == 2) {
        for (const char* suffix : {"/d/f/dict.qidx", "/d/f/dict.sidx"}) {
          const auto it = dictionaryFs.files.find(suffix);
          if (it == dictionaryFs.files.end() || it->second->empty()) continue;
          local = local * 1103515245 + 12345;
          if (damage == 1) {
            (*it->second)[local % it->second->size()] ^= static_cast<uint8_t>(local >> 16);
          } else {
            it->second->resize(local % it->second->size());
          }
        }
      }
      if (damage == 3) dictionaryFs.files.erase("/d/f/dict.sidx");
      r.neededAfter = d.needsIndex();
      for (const auto& q : queries) {
        std::string definition, headword;
        decltype(lookupResult) lr{};
        const bool hit = d.lookup(q.c_str(), definition, headword, &lr);
        r.results.push_back(std::to_string(hit) + "|" + std::to_string(static_cast<int>(lr)) + "|" + headword + "|" +
                            definition);
      }
      return r;
    };

    dictionaryFs = copyOf(start);
    OldDictionary oldD;
    const Run a = run(oldD, OldDictionary::IndexResult{}, OldDictionary::LookupResult{});
    const DictionaryTestFs oldFs = copyOf(dictionaryFs);
    dictionaryFs = copyOf(start);
    Dictionary newD;
    const Run b = run(newD, Dictionary::IndexResult{}, Dictionary::LookupResult{});

    const bool same = a.opened == b.opened && a.html == b.html && a.neededIndex == b.neededIndex &&
                      a.built == b.built && a.indexResult == b.indexResult && a.neededAfter == b.neededAfter &&
                      sameFiles(oldFs, dictionaryFs);
    size_t firstDiff = SIZE_MAX;
    for (size_t i = 0; i < a.results.size(); ++i) {
      if (a.results[i] != b.results[i]) {
        firstDiff = i;
        break;
      }
    }
    if (!same || firstDiff != SIZE_MAX) {
      ADD_FAILURE() << "case " << c << " words " << words.size() << " damage " << damage << (same ? "" : " state/files")
                    << (firstDiff == SIZE_MAX ? ""
                                              : " lookup '" + queries[firstDiff] + "': " + a.results[firstDiff] +
                                                    " vs " + b.results[firstDiff]);
      if (++diffs == 5) return;
    }
  }
}
