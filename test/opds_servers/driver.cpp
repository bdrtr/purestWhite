// Oracle for the Rust port of src/OpdsServerStore's JSON: runs one scenario through the store's
// loadFromFile and addServer (so its saveToFile, its resave of a legacy password, and its fromJson
// and toJson over ArduinoJson 7.4.2, the firmware's; over the shim, fromJsonText/toJsonText) and
// prints each answer. The file is test/json_oracle/stubs/HalStorage.h's; the obfuscation is its
// stubs/ObfuscationFake.cpp's. make_corpus.py builds it over the C++ store and PersistableStore
// from git (the store in the tree is now the Rust shim, and PersistableStore has changed with it)
// and keeps the transcript as the Rust tests' golden corpus:
// rust/crates/bembeyaz-store/testdata/opds-servers-corpus.txt. check_shim.sh builds it over the
// shim; the driver is the same for both.
//
//   load <json>       loadFromFile with <json> as the file  = ok|fail <servers> | <written json>
//   add <8 fields>    addServer: name url username password header1 name/value header2
//                     name/value, each in hex                = ok|fail <servers> | <written json>
//   long <n>          load {"servers":[{"name":"<n n's>"}]}, then add a server named n n's
//                                                         = <load> | <add>, each ok|fail
//                                                           <count> <first name's length> <written
//                                                           json's length>
//
// <servers> is the count, then one space-separated item per server: its 8 fields in hex ("-" when
// empty), comma-separated; <written json> is what the store wrote last in the step ("-" for
// nothing): a load writes only when it resaves.
#include <cstdio>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "HalStorage.h"
#include "OpdsServerStore.h"
#include "driver_util.h"

namespace {
std::string serversOf(const OpdsServerStore& s) {
  std::string out = std::to_string(s.getCount());
  for (const OpdsServer& v : s.getServers()) {
    const HttpHeader& a = v.customHeaders[0];
    const HttpHeader& b = v.customHeaders[1];
    out += " " + hexOf(v.name) + "," + hexOf(v.url) + "," + hexOf(v.username) + "," + hexOf(v.password) + "," +
           hexOf(a.name) + "," + hexOf(a.value) + "," + hexOf(b.name) + "," + hexOf(b.value);
  }
  return out;
}

std::string written() { return storageWritten.empty() ? "-" : storageWritten; }

void answer(const bool ok, const OpdsServerStore& s) {
  std::printf("  = %s %s | %s\n", ok ? "ok" : "fail", serversOf(s).c_str(), written().c_str());
}

bool load(OpdsServerStore& s, const std::string& file) {
  storageFile = file;
  storageWritten.clear();
  return s.loadFromFile();
}

std::string lengths(const bool ok, const OpdsServerStore& s) {
  const size_t first = s.getCount() ? s.getServers().front().name.size() : 0;
  return std::string(ok ? "ok " : "fail ") + std::to_string(s.getCount()) + " " + std::to_string(first) + " " +
         std::to_string(storageWritten.size());
}

void longs(OpdsServerStore& s, const size_t n) {
  const bool loaded = load(s, "{\"servers\":[{\"name\":\"" + std::string(n, 'n') + "\"}]}");
  const std::string first = lengths(loaded, s);
  OpdsServer server;
  server.name = std::string(n, 'n');
  storageWritten.clear();
  const bool added = s.addServer(server);
  std::printf("  = %s | %s\n", first.c_str(), lengths(added, s).c_str());
}

void add(OpdsServerStore& s, const std::vector<std::string>& a) {
  OpdsServer server;
  server.name = bytesOf(a.at(1));
  server.url = bytesOf(a.at(2));
  server.username = bytesOf(a.at(3));
  server.password = bytesOf(a.at(4));
  server.customHeaders[0] = {bytesOf(a.at(5)), bytesOf(a.at(6))};
  server.customHeaders[1] = {bytesOf(a.at(7)), bytesOf(a.at(8))};
  storageWritten.clear();
  answer(s.addServer(server), s);
}
}  // namespace

int main() {
  OpdsServerStore& store = OPDS_STORE;
  std::string line;
  while (std::getline(std::cin, line)) {
    std::vector<std::string> a;
    std::stringstream in(line);
    for (std::string w; in >> w;) a.push_back(w);
    if (a.empty()) continue;
    std::printf("%s\n", line.c_str());
    if (a[0] == "load") {
      answer(load(store, bytesOf(a.at(1))), store);
    } else if (a[0] == "add") {
      add(store, a);
    } else if (a[0] == "long") {
      longs(store, std::stoul(a.at(1)));
    }
  }
  return 0;
}
