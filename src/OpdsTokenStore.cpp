#include "OpdsTokenStore.h"

#include <Logging.h>
#include <StoreText.h>
#include <bembeyaz/store_records.h>

#include <algorithm>

namespace {
// The JSON is Rust's (bembeyaz_store::records), and so is which entries are kept (an entry needs its
// URL; the tokens decoded, at most BB_STORE_RECORDS_MOST_TOKEN bytes; at most MAX_SERVERS); this
// file obfuscates the tokens for saving.
void copyEntry(const bb_store_text* fields, OpdsTokenStore::Entry& entry) {
  entry.url = storeString(fields[BB_STORE_OPDS_TOKEN_URL]);
  entry.tokens.accessToken = storeString(fields[BB_STORE_OPDS_TOKEN_ACCESS]);
  entry.tokens.refreshToken = storeString(fields[BB_STORE_OPDS_TOKEN_REFRESH]);
  entry.tokens.refreshUrl = storeString(fields[BB_STORE_OPDS_TOKEN_REFRESH_URL]);
}
}  // namespace

bool OpdsTokenStore::toJsonText(String& json) const {
  ObfuscatedTexts obfuscated;
  std::vector<bb_store_text> fields(entries.size() * BB_STORE_OPDS_TOKEN_FIELDS);
  for (size_t i = 0; i < entries.size(); i++) {
    bb_store_text* record = &fields[i * BB_STORE_OPDS_TOKEN_FIELDS];
    record[BB_STORE_OPDS_TOKEN_URL] = storeText(entries[i].url);
    record[BB_STORE_OPDS_TOKEN_ACCESS_OBF] = obfuscated.add(entries[i].tokens.accessToken);
    record[BB_STORE_OPDS_TOKEN_REFRESH_OBF] = obfuscated.add(entries[i].tokens.refreshToken);
    record[BB_STORE_OPDS_TOKEN_REFRESH_URL] = storeText(entries[i].tokens.refreshUrl);
  }
  if (!obfuscated.ok()) return false;
  StoreJsonSink sink{json, true};
  return bb_store_records_write(BB_STORE_RECORDS_OPDS_TOKENS, fields.data(), entries.size(), appendStoreJson, &sink) &&
         sink.ok;
}

const char* OpdsTokenStore::fromJsonText(const String& json) {
  static_assert(MAX_SERVERS == BB_STORE_RECORDS_MOST_OPDS_TOKENS, "the store's limit is Rust's");
  std::vector<Entry> read;
  LoadedRecords<Entry> loaded{read, copyEntry};
  const char* error = bb_store_records_load(json.c_str(), json.length(), BB_STORE_RECORDS_OPDS_TOKENS,
                                            decodeStoredSecret, addRecord<Entry>, &loaded);
  if (error) return error;
  entries = std::move(read);
  LOG_DBG("OPDSTOK", "Loaded %zu OPDS token entries", entries.size());
  return nullptr;
}

OpdsTokens OpdsTokenStore::get(const std::string& url) const {
  for (const auto& entry : entries) {
    if (entry.url == url) return entry.tokens;
  }
  return {};
}

bool OpdsTokenStore::put(const std::string& url, const OpdsTokens& tokens) {
  const auto it = std::find_if(entries.begin(), entries.end(), [&](const Entry& e) { return e.url == url; });

  if (tokens.empty()) {
    // Clearing: drop the entry if present; nothing to write otherwise.
    if (it == entries.end()) return true;
    entries.erase(it);
    return saveToFile();
  }

  if (it != entries.end()) {
    if (it->tokens.accessToken == tokens.accessToken && it->tokens.refreshToken == tokens.refreshToken &&
        it->tokens.refreshUrl == tokens.refreshUrl) {
      return true;  // unchanged: skip the SD write
    }
    it->tokens = tokens;
    return saveToFile();
  }

  if (entries.size() >= MAX_SERVERS) {
    // Full: evict the oldest entry so a new server can still be remembered.
    entries.erase(entries.begin());
  }
  entries.push_back(Entry{url, tokens});
  return saveToFile();
}
