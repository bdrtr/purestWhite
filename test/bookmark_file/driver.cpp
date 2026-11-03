// Oracle for the Rust port of src/util/BookmarkFile's JSON (a book's bookmarks file): runs one
// scenario through BookmarkFile::load and save (so their checks of each bookmark and the file steps
// of PersistableStoreBase, over ArduinoJson 7.4.2, the firmware's; over the shim, Rust) and prints
// each answer. The file is test/json_oracle/stubs/HalStorage.h's.
//
// Each answer also holds the bookmark layers the Rust port deals in, worked out here with
// ArduinoJson: what the file read holds (read) and what save was given to write (save). The Rust
// tests replay those; check_shim.sh checks the rest.
//
// make_corpus.py builds it over the C++ BookmarkFile and PersistableStore from git (the one in the
// tree is now the Rust shim) and keeps the transcript as the Rust tests' golden corpus:
// rust/crates/bembeyaz-store/testdata/bookmark-file-corpus.txt. check_shim.sh builds it over the
// shim; the driver is the same for both.
//
//   load <json>          BookmarkFile::load with <json> as the file, then save of what it loaded
//   save <bookmark>...   BookmarkFile::save of the bookmarks given (each as in a layer)
//   long <n>             load a file whose first xpath is n letters, then save it
//                        = <load> <save> <count> <written json's length>, where <load> and
//                        <save> are ok|fail
//
// Each step but long answers = <load>,<save> (load: both; save: - and save's) <bookmarks> |
// <written json> | read <layer> | save <layer>, where <bookmarks> is the list held after the step,
// <written json> is what was written ("-" for nothing), the read layer is "-" (no file read), err
// <name>, or the file's bookmarks, and the save layer is "-" (nothing written) or the bookmarks
// given to write. A list is its count, then one space-separated entry per bookmark:
// xpath,summary,name (hex), percentage (the float's bits in hex), si,pc,pp (decimal), vo (decimal,
// or "-" when it has none). The file's are read as the store reads them: each field `| ""`, `|
// 0.0f` or `| uint16_t(0)`, the name not yet cut to MAX_NAME_LENGTH, vo when not null as `|
// uint32_t(0)`.
#include <ArduinoJson.h>

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "BookmarkEntry.h"
#include "HalStorage.h"
#include "driver_util.h"
#include "util/BookmarkFile.h"

namespace {
const std::string kBook = "/Kitaplar/b.epub";

std::string bitsOf(const float f) {
  uint32_t b;
  std::memcpy(&b, &f, sizeof(b));
  char out[9];
  std::snprintf(out, sizeof(out), "%08x", static_cast<unsigned>(b));
  return out;
}

std::string entryOf(const BookmarkEntry& b) {
  return hexOf(b.xpath) + "," + hexOf(b.summary) + "," + hexOf(b.name) + "," + bitsOf(b.percentage) + "," +
         std::to_string(b.computedSpineIndex) + "," + std::to_string(b.computedChapterPageCount) + "," +
         std::to_string(b.computedChapterProgress) + "," +
         (b.hasVisibleTextOffset ? std::to_string(b.visibleTextOffset) : std::string("-"));
}

std::string listOf(const std::vector<BookmarkEntry>& list) {
  std::string out = std::to_string(list.size());
  for (const BookmarkEntry& b : list) out += " " + entryOf(b);
  return out;
}

std::string readLayer(const std::string& file) {
  JsonDocument doc;
  const DeserializationError error = deserializeJson(doc, file.c_str());
  if (error) return std::string("err ") + error.c_str();
  std::vector<BookmarkEntry> list;
  for (const JsonVariantConst o : doc["bookmarks"].as<JsonArrayConst>()) {
    BookmarkEntry b;
    b.xpath = o["xpath"] | "";
    b.percentage = o["percentage"] | static_cast<float>(0);
    b.summary = o["summary"] | "";
    b.name = o["name"] | "";
    b.computedSpineIndex = o["si"] | static_cast<uint16_t>(0);
    b.computedChapterPageCount = o["pc"] | static_cast<uint16_t>(0);
    b.computedChapterProgress = o["pp"] | static_cast<uint16_t>(0);
    if (!o["vo"].isNull()) {
      b.visibleTextOffset = o["vo"] | static_cast<uint32_t>(0);
      b.hasVisibleTextOffset = true;
    }
    list.push_back(b);
  }
  return listOf(list);
}

BookmarkEntry entryFrom(const std::string& item) {
  std::vector<std::string> f;
  std::stringstream in(item);
  for (std::string part; std::getline(in, part, ',');) f.push_back(part);
  BookmarkEntry b;
  b.xpath = bytesOf(f.at(0));
  b.summary = bytesOf(f.at(1));
  b.name = bytesOf(f.at(2));
  const uint32_t bits = static_cast<uint32_t>(std::stoul(f.at(3), nullptr, 16));
  std::memcpy(&b.percentage, &bits, sizeof(bits));
  b.computedSpineIndex = static_cast<uint16_t>(std::stoul(f.at(4)));
  b.computedChapterPageCount = static_cast<uint16_t>(std::stoul(f.at(5)));
  b.computedChapterProgress = static_cast<uint16_t>(std::stoul(f.at(6)));
  b.hasVisibleTextOffset = f.at(7) != "-";
  if (b.hasVisibleTextOffset) b.visibleTextOffset = static_cast<uint32_t>(std::stoul(f.at(7)));
  return b;
}

void answer(const std::string& result, const std::vector<BookmarkEntry>& held, const std::string& read,
            const std::vector<BookmarkEntry>& given) {
  const std::string written = storageWritten.empty() ? "-" : storageWritten;
  const std::string save = storageWritten.empty() ? "-" : listOf(given);
  std::printf("  = %s %s | %s | read %s | save %s\n", result.c_str(), listOf(held).c_str(), written.c_str(),
              read.c_str(), save.c_str());
}

const char* okOf(const bool ok) { return ok ? "ok" : "fail"; }

void longs(const size_t n) {
  storageFile = "{\"bookmarks\":[{\"xpath\":\"" + std::string(n, 'x') + "\"}]}";
  storageWritten.clear();
  std::vector<BookmarkEntry> list;
  const bool loaded = BookmarkFile::load(kBook, list);
  const bool saved = BookmarkFile::save(kBook, list);
  std::printf("  = %s %s %zu %zu\n", okOf(loaded), okOf(saved), list.size(), storageWritten.size());
}

void step(const std::vector<std::string>& a) {
  storageWritten.clear();
  std::vector<BookmarkEntry> list;
  if (a[0] == "load") {
    const std::string file = bytesOf(a.at(1));
    storageFile = file;
    const bool loaded = BookmarkFile::load(kBook, list);
    const bool saved = BookmarkFile::save(kBook, list);
    // The store reads the file as a C string, as this does.
    answer(std::string(okOf(loaded)) + "," + okOf(saved), list, readLayer(file.c_str()), list);
  } else if (a[0] == "save") {
    for (size_t i = 1; i < a.size(); i++) list.push_back(entryFrom(a[i]));
    answer(std::string("-,") + okOf(BookmarkFile::save(kBook, list)), list, "-", list);
  } else if (a[0] == "long") {
    longs(std::stoul(a.at(1)));
  }
}
}  // namespace

int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    std::vector<std::string> a;
    std::stringstream in(line);
    for (std::string w; in >> w;) a.push_back(w);
    if (a.empty()) continue;
    std::printf("%s\n", line.c_str());
    step(a);
  }
  return 0;
}
