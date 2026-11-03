#include "LcpPassphraseStore.h"

#include <Logging.h>
#include <ObfuscationUtils.h>
#include <StoreText.h>
#include <bembeyaz/store_records.h>

#include <algorithm>

namespace {
// A hex sha256 is exactly 64 chars; anything longer is a malformed file.
constexpr size_t MAX_HASH_CHARS = 64;

// The JSON is Rust's (bembeyaz_store::records); this file obfuscates the hashes and decides which
// entries to keep, as the ArduinoJson version did.

// An entry of a record with a provider and a hash that decodes.
bool parseEntry(const bb_store_text* fields, LcpPassphraseStore::Entry& entry) {
  entry.provider = storeString(fields[BB_STORE_LCP_PASSPHRASE_PROVIDER]);
  if (entry.provider.empty()) return false;
  bool ok = false;
  bool tooLong = false;
  entry.userKeyHex = obfuscation::deobfuscateFromBase64(storeString(fields[BB_STORE_LCP_PASSPHRASE_KEY_OBF]).c_str(),
                                                        MAX_HASH_CHARS, &ok, &tooLong);
  return !entry.userKeyHex.empty();
}
}  // namespace

bool LcpPassphraseStore::toJsonText(String& json) const {
  ObfuscatedTexts obfuscated;
  std::vector<bb_store_text> fields(entries.size() * BB_STORE_LCP_PASSPHRASE_FIELDS);
  for (size_t i = 0; i < entries.size(); i++) {
    bb_store_text* record = &fields[i * BB_STORE_LCP_PASSPHRASE_FIELDS];
    record[BB_STORE_LCP_PASSPHRASE_PROVIDER] = storeText(entries[i].provider);
    record[BB_STORE_LCP_PASSPHRASE_KEY_OBF] = obfuscated.add(entries[i].userKeyHex);
  }
  if (!obfuscated.ok()) return false;
  StoreJsonSink sink{json, true};
  return bb_store_records_write(BB_STORE_RECORDS_LCP_PASSPHRASES, fields.data(), entries.size(), appendStoreJson,
                                &sink) &&
         sink.ok;
}

const char* LcpPassphraseStore::fromJsonText(const String& json) {
  std::vector<Entry> read;
  KeptRecords<Entry> kept{read, MAX_PROVIDERS, parseEntry};
  const char* error =
      bb_store_records_read(json.c_str(), json.length(), BB_STORE_RECORDS_LCP_PASSPHRASES, keepRecord<Entry>, &kept);
  if (error) return error;
  entries = std::move(read);
  LOG_DBG("LCPPASS", "Loaded %zu LCP passphrase entries", entries.size());
  return nullptr;
}

std::string LcpPassphraseStore::get(const std::string& provider) const {
  for (const auto& entry : entries) {
    if (entry.provider == provider) return entry.userKeyHex;
  }
  return {};
}

bool LcpPassphraseStore::put(const std::string& provider, const std::string& userKeyHex) {
  const auto it = std::find_if(entries.begin(), entries.end(), [&](const Entry& e) { return e.provider == provider; });

  if (userKeyHex.empty()) {
    if (it == entries.end()) return true;
    entries.erase(it);
    return saveToFile();
  }

  if (it != entries.end()) {
    if (it->userKeyHex == userKeyHex) return true;  // unchanged: skip the SD write
    it->userKeyHex = userKeyHex;
    return saveToFile();
  }

  if (entries.size() >= MAX_PROVIDERS) {
    // Full: evict the oldest entry so a new library can still be remembered.
    entries.erase(entries.begin());
  }
  entries.push_back(Entry{provider, userKeyHex});
  return saveToFile();
}
