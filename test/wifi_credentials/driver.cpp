// Oracle for the Rust port of src/WifiCredentialStore's JSON: runs one scenario through the store's
// loadFromFile, addCredential, setLastConnectedSsid and removeCredential (so its saveToFile, its
// checks of each password's length and CRC-32, its resave, and its fromJson and toJson over
// ArduinoJson 7.4.2, the firmware's; over the shim, fromJsonText/toJsonText) and prints each answer.
// The file is test/json_oracle/stubs/HalStorage.h's; the obfuscation is its ObfuscationFake.cpp's.
//
// Each answer also holds the stored-text layers the Rust port deals in, worked out here from the
// same inputs with ArduinoJson: what a file holds for the store to decide on (read), and what the
// store gives to be written (save). The Rust tests replay those; check_shim.sh checks the rest.
//
// make_corpus.py builds it over the C++ store, PersistableStore and StoredPassword from git (the
// store in the tree is now the Rust shim) and keeps the transcript as the Rust tests' golden corpus:
// rust/crates/bembeyaz-store/testdata/wifi-credentials-corpus.txt. check_shim.sh builds it over the
// shim; the driver is the same for both.
//
//   load <json>              loadFromFile with <json> as the file
//   add <ssid> <password>    addCredential (both in hex)
//   last <ssid>              setLastConnectedSsid
//   remove <ssid>            removeCredential
//   long <n>                 load {"lastConnectedSsid":"<n s's>"}, then add a network named n s's
//
// Each answers = ok|fail <networks> | <written json> | read <layer> | save <layer>, but long:
// = <load> | <add>, each ok|fail <last connected length> <count> <written json's length>.
// <networks> is the last connected SSID, the count, then one space-separated item per network
// (ssid,password in hex, "-" when empty); <written json> is what the store wrote in the step ("-"
// for nothing). The read layer ("-" for a step that reads no file) is err <name>, or the last
// connected SSID, the count of "credentials" items, then one item per item:
// ssid,password_obf,password,<password_len>,<password_crc32>, each number as kind:value (kind 0
// missing, 1 a u32, 2 another type). The save layer ("-" when nothing was written) is the last
// connected SSID and one item per network: ssid,password_obf,password_len,password_crc32.
#include <ArduinoJson.h>
#include <CredentialIntegrity.h>
#include <ObfuscationUtils.h>

#include <cstdio>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "HalStorage.h"
#include "WifiCredentialStore.h"
#include "driver_util.h"

namespace {
std::string numberOf(const JsonVariantConst v) {
  if (v.isNull()) return "0:0";
  if (v.is<uint32_t>()) return "1:" + std::to_string(v.as<uint32_t>());
  return "2:0";
}

std::string readLayer(const std::string& file) {
  JsonDocument doc;
  const DeserializationError error = deserializeJson(doc, file.c_str());
  if (error) return std::string("err ") + error.c_str();
  const JsonArrayConst items = doc["credentials"].as<JsonArrayConst>();
  std::string out = hexOf(doc["lastConnectedSsid"] | "") + " " + std::to_string(items.size());
  for (const JsonVariantConst item : items) {
    out += " " + hexOf(item["ssid"] | "") + "," + hexOf(item["password_obf"] | "") + "," +
           hexOf(item["password"] | "") + "," + numberOf(item["password_len"]) + "," + numberOf(item["password_crc32"]);
  }
  return out;
}

std::string saveLayer(const WifiCredentialStore& s) {
  if (storageWritten.empty()) return "-";
  std::string out = hexOf(s.getLastConnectedSsid());
  for (size_t i = 0; i < s.getCredentialCount(); i++) {
    const WifiCredential c = *s.getCredentialAt(i);
    out += " " + hexOf(c.ssid) + "," + hexOf(obfuscation::obfuscateToBase64(c.password).c_str()) + "," +
           std::to_string(c.password.size()) + "," + std::to_string(credential_integrity::crc32(c.password));
  }
  return out;
}

std::string networksOf(const WifiCredentialStore& s) {
  std::string out = hexOf(s.getLastConnectedSsid()) + " " + std::to_string(s.getCredentialCount());
  for (size_t i = 0; i < s.getCredentialCount(); i++) {
    const WifiCredential c = *s.getCredentialAt(i);
    out += " " + hexOf(c.ssid) + "," + hexOf(c.password);
  }
  return out;
}

void answer(const bool ok, const WifiCredentialStore& s, const std::string& read) {
  std::printf("  = %s %s | %s | read %s | save %s\n", ok ? "ok" : "fail", networksOf(s).c_str(),
              storageWritten.empty() ? "-" : storageWritten.c_str(), read.c_str(), saveLayer(s).c_str());
}

bool load(WifiCredentialStore& s, const std::string& file) {
  storageFile = file;
  storageWritten.clear();
  return s.loadFromFile();
}

std::string lengths(const bool ok, const WifiCredentialStore& s) {
  return std::string(ok ? "ok " : "fail ") + std::to_string(s.getLastConnectedSsid().size()) + " " +
         std::to_string(s.getCredentialCount()) + " " + std::to_string(storageWritten.size());
}

void longs(WifiCredentialStore& s, const size_t n) {
  const bool loaded = load(s, "{\"lastConnectedSsid\":\"" + std::string(n, 's') + "\"}");
  const std::string first = lengths(loaded, s);
  storageWritten.clear();
  const bool added = s.addCredential(std::string(n, 's'), "p");
  std::printf("  = %s | %s\n", first.c_str(), lengths(added, s).c_str());
}

void step(WifiCredentialStore& s, const std::vector<std::string>& a) {
  if (a[0] == "load") {
    const std::string file = bytesOf(a.at(1));
    const bool ok = load(s, file);
    // The store reads the file as a C string, as this does.
    answer(ok, s, readLayer(file.c_str()));
    return;
  }
  storageWritten.clear();
  bool ok = false;
  if (a[0] == "add") {
    ok = s.addCredential(bytesOf(a.at(1)), bytesOf(a.at(2)));
  } else if (a[0] == "last") {
    s.setLastConnectedSsid(bytesOf(a.at(1)));
    ok = true;
  } else if (a[0] == "remove") {
    ok = s.removeCredential(bytesOf(a.at(1)));
  }
  answer(ok, s, "-");
}
}  // namespace

int main() {
  WifiCredentialStore& store = WIFI_STORE;
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
