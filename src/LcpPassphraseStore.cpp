#include "LcpPassphraseStore.h"

#include <Logging.h>
#include <StoreText.h>
#include <bembeyaz/store_records.h>

#include <algorithm>

namespace {
// The JSON is Rust's (bembeyaz_store::records), and so is which entries are kept (an entry needs its
// provider and a hash that decodes, at most BB_STORE_RECORDS_MOST_HASH bytes; at most
// MAX_PROVIDERS); this file obfuscates the hashes for saving.
void copyEntry(const bb_store_text* fields, LcpPassphraseStore::Entry& entry) {
  entry.provider = storeString(fields[BB_STORE_LCP_PASSPHRASE_PROVIDER]);
  entry.userKeyHex = storeString(fields[BB_STORE_LCP_PASSPHRASE_KEY]);
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
  static_assert(MAX_PROVIDERS == BB_STORE_RECORDS_MOST_LCP_PASSPHRASES, "the store's limit is Rust's");
  std::vector<Entry> read;
  LoadedRecords<Entry> loaded{read, copyEntry};
  const char* error = bb_store_records_load(json.c_str(), json.length(), BB_STORE_RECORDS_LCP_PASSPHRASES,
                                            decodeStoredSecret, addRecord<Entry>, &loaded);
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
