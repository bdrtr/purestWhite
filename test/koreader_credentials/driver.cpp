// Oracle for the Rust port of lib/KOReaderSync/KOReaderCredentialStore's JSON: runs one scenario
// through the store's loadFromFile, setters with saveToFile, and clearCredentials (so its upgrades
// of older configs, its checks of each setting, its resave, and its fromJson and toJson over
// ArduinoJson 7.4.2, the firmware's; over the shim, fromJsonText/toJsonText) and prints each answer.
// The file is test/json_oracle/stubs/HalStorage.h's; the obfuscation is its ObfuscationFake.cpp's.
//
// Each answer also holds the stored-text layers the Rust port deals in, worked out here from the
// same inputs with ArduinoJson: what a file holds for the store to decide on (read), and what the
// store gives to be written (save). The Rust tests replay those; check_shim.sh checks the rest.
//
// make_corpus.py builds it over the C++ store, PersistableStore and StoredPassword from git (the
// store in the tree is now the Rust shim) and keeps the transcript as the Rust tests' golden corpus:
// rust/crates/bembeyaz-store/testdata/koreader-credentials-corpus.txt. check_shim.sh builds it over
// the shim; the driver is the same for both.
//
//   load <json>       loadFromFile with <json> as the file
//   set <12 fields>   setCredentials(user, password), setServerUrl, setServerType, setMatchMethod,
//                     setSendMetadata, setSyncClippings, setSyncBehavior, setCustomHeader(0 and 1),
//                     then saveToFile; texts in hex, numbers and bools (0|1) in decimal
//   clear             clearCredentials
//   long <n>          load {"username":"<n u's>","password":"p"}, then set a URL of n u's and save
//
// Each step answers = ok|fail <settings> | <written json> | read <layer> | save <layer>, but long:
// = <load> | <set>, each ok|fail <user length> <url length> <written json's length>. <settings> is
// user,password,url (hex) type,method,metadata,clippings,behavior (decimal) then the two headers'
// name,value (hex); <written json> is what the store wrote in the step ("-" for nothing). The read
// layer ("-" for a step that reads no file) is err <name>, or username,password_obf,password,
// serverUrl (hex), then cfgVersion, serverType, matchMethod and syncBehavior each as kind:value:byte
// (kind 0 missing, 1 a u32, 2 another type; byte is as<uint8_t>()), sendMetadata and syncClippings
// (| false, 0|1), the count of "headers" read (at most 2) and each one's name,value_obf (hex). The
// save layer ("-" when nothing was written) is username,password_obf,serverUrl (hex),
// cfgVersion,serverType,matchMethod,sendMetadata,syncClippings,syncBehavior (decimal) and both
// headers' name,value_obf (hex).
#include <ArduinoJson.h>
#include <ObfuscationUtils.h>

#include <cstdio>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "HalStorage.h"
#include "KOReaderCredentialStore.h"
#include "driver_util.h"

namespace {
// The store's CONFIG_VERSION, which every save writes.
constexpr unsigned kConfigVersion = 3;

std::string numberOf(const JsonVariantConst v) {
  const std::string byte = std::to_string(v.as<uint8_t>());
  if (v.isNull()) return "0:0:" + byte;
  if (v.is<uint32_t>()) return "1:" + std::to_string(v.as<uint32_t>()) + ":" + byte;
  return "2:0:" + byte;
}

std::string readLayer(const std::string& file) {
  JsonDocument doc;
  const DeserializationError error = deserializeJson(doc, file.c_str());
  if (error) return std::string("err ") + error.c_str();
  std::string out = hexOf(doc["username"] | "") + "," + hexOf(doc["password_obf"] | "") + "," +
                    hexOf(doc["password"] | "") + "," + hexOf(doc["serverUrl"] | "");
  for (const char* key : {"cfgVersion", "serverType", "matchMethod", "syncBehavior"}) {
    out += " " + numberOf(doc[key]);
  }
  out += " " + std::to_string(doc["sendMetadata"] | false) + " " + std::to_string(doc["syncClippings"] | false);
  std::string headers;
  size_t count = 0;
  for (const JsonVariantConst h : doc["headers"].as<JsonArrayConst>()) {
    if (count == KOReaderCredentialStore::MAX_CUSTOM_HEADERS) break;
    headers += " " + hexOf(h["name"] | "") + "," + hexOf(h["value_obf"] | "");
    count++;
  }
  return out + " " + std::to_string(count) + headers;
}

std::string obf(const std::string& plain) { return obfuscation::obfuscateToBase64(plain).c_str(); }

std::string saveLayer(const KOReaderCredentialStore& s) {
  if (storageWritten.empty()) return "-";
  std::string out = hexOf(s.getUsername()) + "," + hexOf(obf(s.getPassword())) + "," + hexOf(s.getServerUrl()) + " " +
                    std::to_string(kConfigVersion) + "," + std::to_string(static_cast<unsigned>(s.getServerType())) +
                    "," + std::to_string(static_cast<unsigned>(s.getMatchMethod())) + "," +
                    std::to_string(s.getSendMetadata()) + "," + std::to_string(s.getSyncClippings()) + "," +
                    std::to_string(static_cast<unsigned>(s.getSyncBehavior()));
  for (const HttpHeader& h : s.getCustomHeaders()) out += " " + hexOf(h.name) + "," + hexOf(obf(h.value));
  return out;
}

std::string settingsOf(const KOReaderCredentialStore& s) {
  std::string out = hexOf(s.getUsername()) + "," + hexOf(s.getPassword()) + "," + hexOf(s.getServerUrl()) + " " +
                    std::to_string(static_cast<unsigned>(s.getServerType())) + "," +
                    std::to_string(static_cast<unsigned>(s.getMatchMethod())) + "," +
                    std::to_string(s.getSendMetadata()) + "," + std::to_string(s.getSyncClippings()) + "," +
                    std::to_string(static_cast<unsigned>(s.getSyncBehavior()));
  for (const HttpHeader& h : s.getCustomHeaders()) out += " " + hexOf(h.name) + "," + hexOf(h.value);
  return out;
}

void answer(const bool ok, const KOReaderCredentialStore& s, const std::string& read) {
  std::printf("  = %s %s | %s | read %s | save %s\n", ok ? "ok" : "fail", settingsOf(s).c_str(),
              storageWritten.empty() ? "-" : storageWritten.c_str(), read.c_str(), saveLayer(s).c_str());
}

bool load(KOReaderCredentialStore& s, const std::string& file) {
  storageFile = file;
  storageWritten.clear();
  return s.loadFromFile();
}

void set(KOReaderCredentialStore& s, const std::vector<std::string>& a) {
  s.setCredentials(bytesOf(a.at(1)), bytesOf(a.at(2)));
  s.setServerUrl(bytesOf(a.at(3)));
  s.setServerType(static_cast<KOReaderServerType>(std::stoul(a.at(4))));
  s.setMatchMethod(static_cast<DocumentMatchMethod>(std::stoul(a.at(5))));
  s.setSendMetadata(a.at(6) == "1");
  s.setSyncClippings(a.at(7) == "1");
  s.setSyncBehavior(static_cast<KOReaderSyncBehavior>(std::stoul(a.at(8))));
  s.setCustomHeader(0, bytesOf(a.at(9)), bytesOf(a.at(10)));
  s.setCustomHeader(1, bytesOf(a.at(11)), bytesOf(a.at(12)));
}

std::string lengths(const bool ok, const KOReaderCredentialStore& s) {
  return std::string(ok ? "ok " : "fail ") + std::to_string(s.getUsername().size()) + " " +
         std::to_string(s.getServerUrl().size()) + " " + std::to_string(storageWritten.size());
}

void longs(KOReaderCredentialStore& s, const size_t n) {
  const bool loaded = load(s, "{\"username\":\"" + std::string(n, 'u') + "\",\"password\":\"p\"}");
  const std::string first = lengths(loaded, s);
  s.setServerUrl(std::string(n, 'u'));
  storageWritten.clear();
  const bool saved = s.saveToFile();
  std::printf("  = %s | %s\n", first.c_str(), lengths(saved, s).c_str());
}

void step(KOReaderCredentialStore& s, const std::vector<std::string>& a) {
  if (a[0] == "load") {
    const std::string file = bytesOf(a.at(1));
    const bool ok = load(s, file);
    // The store reads the file as a C string, as this does.
    answer(ok, s, readLayer(file.c_str()));
    return;
  }
  storageWritten.clear();
  bool ok = true;
  if (a[0] == "set") {
    set(s, a);
    ok = s.saveToFile();
  } else if (a[0] == "clear") {
    s.clearCredentials();
  }
  answer(ok, s, "-");
}
}  // namespace

int main() {
  KOReaderCredentialStore& store = KOREADER_STORE;
  std::string line;
  while (std::getline(std::cin, line)) {
    std::vector<std::string> a;
    std::stringstream in(line);
    for (std::string w; in >> w;) a.push_back(w);
    if (a.empty()) continue;
    std::printf("%s\n", line.c_str());
    if (a[0] == "long") {
      longs(store, std::stoul(a.at(1)));
    } else {
      step(store, a);
    }
  }
  return 0;
}
