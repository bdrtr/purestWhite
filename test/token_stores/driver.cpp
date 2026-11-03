// Oracle for the Rust port of three small credential stores' JSON: src/OpdsTokenStore
// (opds_tokens.json), src/LcpPassphraseStore (lcp_passphrases.json) and src/network/LcpDeviceStore
// (lcp_device.json). Each is a list of records, or one record, of texts. It runs one scenario through
// a store's loadFromFile, then saveToFile, or its put/set and saveToFile (so its checks of each entry,
// its limit and eviction, and its fromJson and toJson over ArduinoJson 7.4.2, the firmware's; over
// the shim, fromJsonText/toJsonText) and prints each answer. The file is
// test/json_oracle/stubs/HalStorage.h's; the obfuscation is its ObfuscationFake.cpp's.
//
// Each answer also holds the record layers the Rust port deals in, worked out here with ArduinoJson:
// what the file read holds (read) and what the store wrote (save). The Rust tests replay those;
// check_shim.sh checks the rest.
//
// make_corpus.py builds it over the C++ stores, PersistableStore and StoredPassword from git (the
// stores in the tree are now the Rust shims) and keeps the transcript as the Rust tests' golden
// corpus: rust/crates/bembeyaz-store/testdata/token-stores-corpus.txt. check_shim.sh builds it over
// the shims; the driver is the same for both.
//
//   tokens load <json>                          OpdsTokenStore loadFromFile, then saveToFile
//   tokens put <url> <access> <refresh> <refreshUrl>
//                                               put
//   pass load <json>                            LcpPassphraseStore loadFromFile, then saveToFile
//   pass put <provider> <key>                   put
//   device load <json>                          LcpDeviceStore loadFromFile, then saveToFile
//   device set <id> <priv>                      the two fields set, then saveToFile
//   tokens long <n>, device long <n>            load a file whose first url (for device: whose
//                                               device_id) is n letters, then saveToFile
//                                               = ok|fail (loadFromFile's) <written json's length>
// (every text in hex). Each step but long answers = <answer> | <written json> | read <layer> | save
// <layer>, where <answer> is ok|fail for what put or saveToFile gave (load: loadFromFile's and
// saveToFile's, as l,s), <written json> is what the store wrote last in the step ("-" for nothing),
// and each layer is "-" (no file read, nothing written), err <name>, or the records: their count,
// then one space-separated entry per record, its fields (OpdsTokenStore: url, access_obf,
// refresh_obf, refresh_url; LcpPassphraseStore: provider, key_obf; LcpDeviceStore: device_id,
// priv_obf) read with `| ""` (in the save layer, the whole string, including any NUL), in hex,
// comma-separated.
#include <ArduinoJson.h>

#include <cstdio>
#include <initializer_list>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "HalStorage.h"
#include "LcpPassphraseStore.h"
#include "OpdsTokenStore.h"
#include "driver_util.h"
#include "network/LcpDeviceStore.h"

namespace {
struct Shape {
  const char* list;  // nullptr: the record is the root object
  std::vector<const char*> keys;
};

const Shape kTokens{"tokens", {"url", "access_obf", "refresh_obf", "refresh_url"}};
const Shape kPassphrases{"passphrases", {"provider", "key_obf"}};
const Shape kDevice{nullptr, {"device_id", "priv_obf"}};

// A field as the store reads it (`| ""`, up to a NUL), or (whole) the whole string as written,
// including any NUL.
std::string fieldOf(const JsonVariantConst field, const bool whole) {
  if (!whole) return field | "";
  const JsonString s = field.as<JsonString>();
  return s.c_str() ? std::string(s.c_str(), s.size()) : std::string();
}

std::string record(const JsonVariantConst item, const Shape& shape, const bool whole) {
  std::string out;
  for (const char* key : shape.keys) out += (out.empty() ? "" : ",") + hexOf(fieldOf(item[key], whole));
  return out;
}

// The records a JSON text holds: as the store reads them, or (whole) as they were written.
std::string layer(const std::string& text, const Shape& shape, const bool whole = false) {
  JsonDocument doc;
  const DeserializationError error = deserializeJson(doc, text.c_str());
  if (error) return std::string("err ") + error.c_str();
  if (!shape.list) return "1 " + record(doc.as<JsonVariantConst>(), shape, whole);
  const JsonArrayConst items = doc[shape.list].as<JsonArrayConst>();
  std::string out = std::to_string(items.size());
  for (const JsonVariantConst item : items) out += " " + record(item, shape, whole);
  return out;
}

void answer(const std::string& result, const std::string& read, const Shape& shape) {
  const std::string save = storageWritten.empty() ? "-" : layer(storageWritten, shape, true);
  std::printf("  = %s | %s | read %s | save %s\n", result.c_str(),
              storageWritten.empty() ? "-" : storageWritten.c_str(), read.c_str(), save.c_str());
}

const char* okOf(const bool ok) { return ok ? "ok" : "fail"; }

// loadFromFile, then saveToFile, which shows what the store kept.
template <typename Store>
void load(Store& store, const std::string& file, const Shape& shape) {
  storageFile = file;
  storageWritten.clear();
  const bool loaded = store.loadFromFile();
  const bool saved = store.saveToFile();
  // The store reads the file as a C string, as this does.
  answer(std::string(okOf(loaded)) + "," + okOf(saved), layer(file.c_str(), shape), shape);
}

void longs(const std::vector<std::string>& a) {
  const size_t n = std::stoul(a.at(2));
  storageWritten.clear();
  bool loaded = false;
  if (a[0] == "tokens") {
    storageFile = "{\"tokens\":[{\"url\":\"" + std::string(n, 'u') + "\"}]}";
    loaded = OPDS_TOKENS.loadFromFile();
    OPDS_TOKENS.saveToFile();
  } else {
    storageFile = "{\"device_id\":\"" + std::string(n, 'd') + "\"}";
    loaded = LCP_DEVICE.loadFromFile();
    LCP_DEVICE.saveToFile();
  }
  std::printf("  = %s %zu\n", okOf(loaded), storageWritten.size());
}

void step(const std::vector<std::string>& a) {
  if (a[1] == "long") {
    longs(a);
    return;
  }
  storageWritten.clear();
  if (a[1] == "load") {
    const std::string file = bytesOf(a.at(2));
    if (a[0] == "tokens") load(OPDS_TOKENS, file, kTokens);
    if (a[0] == "pass") load(LCP_PASSPHRASES, file, kPassphrases);
    if (a[0] == "device") load(LCP_DEVICE, file, kDevice);
    return;
  }
  if (a[0] == "tokens") {
    const OpdsTokens tokens{bytesOf(a.at(3)), bytesOf(a.at(4)), bytesOf(a.at(5))};
    answer(okOf(OPDS_TOKENS.put(bytesOf(a.at(2)), tokens)), "-", kTokens);
  } else if (a[0] == "pass") {
    answer(okOf(LCP_PASSPHRASES.put(bytesOf(a.at(2)), bytesOf(a.at(3)))), "-", kPassphrases);
  } else if (a[0] == "device") {
    LCP_DEVICE.deviceId = bytesOf(a.at(2));
    LCP_DEVICE.privHex = bytesOf(a.at(3));
    answer(okOf(LCP_DEVICE.saveToFile()), "-", kDevice);
  }
}
}  // namespace

int main() {
  std::string line;
  while (std::getline(std::cin, line)) {
    std::vector<std::string> a;
    std::stringstream in(line);
    for (std::string w; in >> w;) a.push_back(w);
    if (a.size() < 2) continue;
    std::printf("%s\n", line.c_str());
    step(a);
  }
  return 0;
}
