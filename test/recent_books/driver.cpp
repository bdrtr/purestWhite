// Oracle for the Rust port of src/RecentBooksStore's JSON: runs one scenario through the store's
// reading and writing (fromJson/toJson over ArduinoJson 7.4.2, the firmware's; with -DSHIM,
// fromJsonText/toJsonText) and prints each answer. make_corpus.py builds it over the C++ store
// (from git: the one in the tree is the Rust shim) and keeps the transcript as the Rust tests'
// golden corpus: rust/crates/bembeyaz-store/testdata/recent-books-corpus.txt. check_shim.sh builds
// it with -DSHIM over the shim.
//
//   from <json>                          read the store from <json>; = err <name> |
//                                        = <books> | <written json>
//   add <path> <title> <author> <cover>  addBook (its fields in hex), then = <books> | <written json>
//   long <n>                             the store read from {"books":[{"title":"<n t's>"}]}, then
//                                        a book added with a title of n t's: = <read> | <added>,
//                                        where <read> is err <name> or <count> <first title's
//                                        length> <written json's length>, and <added> the same
//                                        three numbers after the add
//
// <books> is the count, then one space-separated item per book: its four fields in hex ("-" when
// empty), comma-separated; the written JSON is serializeJson's text (one line: it escapes control
// characters).
#define ARDUINOJSON_ENABLE_ARDUINO_STRING 0
#include <ArduinoJson.h>

#include <cstdio>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "RecentBooksStore.h"
#include "driver_util.h"

// The store saves after addBook; the file is not part of its answers.
#ifdef SHIM
bool PersistableStoreBase::writeTextToFile(const char*, const String&) { return true; }
#else
bool PersistableStoreBase::writeDocToFile(const char*, const JsonDocument&) { return true; }
#endif

namespace {
#ifdef SHIM
std::string jsonOf(const RecentBooksStore& s) {
  String json;
  s.toJsonText(json);
  return json.c_str();
}

bool load(RecentBooksStore& s, const std::string& input, std::string& error) {
  const char* name = s.fromJsonText(String(input.c_str()));
  if (name) error = name;
  return !name;
}
#else
std::string jsonOf(const RecentBooksStore& s) {
  JsonDocument doc;
  s.toJson(doc);
  std::string out;
  serializeJson(doc, out);
  return out;
}

bool load(RecentBooksStore& s, const std::string& input, std::string& error) {
  JsonDocument doc;
  const DeserializationError result = deserializeJson(doc, input.c_str());
  if (result) {
    error = result.c_str();
    return false;
  }
  s.fromJson(doc.as<JsonVariantConst>());
  return true;
}
#endif

void answer(const RecentBooksStore& s) {
  std::string books = std::to_string(s.getCount());
  for (const RecentBook& b : s.getBooks()) {
    books += " " + hexOf(b.path) + "," + hexOf(b.title) + "," + hexOf(b.author) + "," + hexOf(b.coverBmpPath);
  }
  std::printf("  = %s | %s\n", books.c_str(), jsonOf(s).c_str());
}
}  // namespace

std::string lengths(const RecentBooksStore& s) {
  return std::to_string(s.getCount()) + " " + std::to_string(s.getBooks().front().title.size()) + " " +
         std::to_string(jsonOf(s).size());
}

void longs(RecentBooksStore& s, const size_t n) {
  std::string error;
  const std::string json = "{\"books\":[{\"title\":\"" + std::string(n, 't') + "\"}]}";
  const std::string read = load(s, json, error) ? lengths(s) : "err " + error;
  s.addBook("/long", std::string(n, 't'), "", "");
  std::printf("  = %s | %s\n", read.c_str(), lengths(s).c_str());
}

int main() {
  RecentBooksStore& store = RECENT_BOOKS;
  std::string line;
  while (std::getline(std::cin, line)) {
    std::vector<std::string> a;
    std::stringstream in(line);
    for (std::string w; in >> w;) a.push_back(w);
    if (a.empty()) continue;
    std::printf("%s\n", line.c_str());
    if (a[0] == "from") {
      std::string error;
      if (!load(store, bytesOf(a.at(1)), error)) {
        std::printf("  = err %s\n", error.c_str());
        continue;
      }
      answer(store);
    } else if (a[0] == "add") {
      store.addBook(bytesOf(a.at(1)), bytesOf(a.at(2)), bytesOf(a.at(3)), bytesOf(a.at(4)));
      answer(store);
    } else if (a[0] == "long") {
      longs(store, std::stoul(a.at(1)));
    }
  }
  return 0;
}
