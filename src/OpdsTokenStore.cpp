#include "OpdsTokenStore.h"

#include <Logging.h>
#include <ObfuscationUtils.h>
#include <StoreText.h>
#include <bembeyaz/store_records.h>

#include <algorithm>

namespace {
// Access/refresh tokens (JWTs) run long; cap the decode to avoid a malformed
// file ballooning the heap.
constexpr size_t MAX_TOKEN_CHARS = 2048;

// The JSON is Rust's (bembeyaz_store::records); this file obfuscates the tokens and decides which
// entries to keep, as the ArduinoJson version did.
std::string decoded(const bb_store_text& obfuscated) {
  bool ok = false;
  bool tooLong = false;
  return obfuscation::deobfuscateFromBase64(storeString(obfuscated).c_str(), MAX_TOKEN_CHARS, &ok, &tooLong);
}

// An entry of a record with a URL; its tokens empty when they do not decode.
bool parseEntry(const bb_store_text* fields, OpdsTokenStore::Entry& entry) {
  entry.url = storeString(fields[BB_STORE_OPDS_TOKEN_URL]);
  if (entry.url.empty()) return false;
  entry.tokens.accessToken = decoded(fields[BB_STORE_OPDS_TOKEN_ACCESS_OBF]);
  entry.tokens.refreshToken = decoded(fields[BB_STORE_OPDS_TOKEN_REFRESH_OBF]);
  entry.tokens.refreshUrl = storeString(fields[BB_STORE_OPDS_TOKEN_REFRESH_URL]);
  return true;
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
  std::vector<Entry> read;
  KeptRecords<Entry> kept{read, MAX_SERVERS, parseEntry};
  const char* error =
      bb_store_records_read(json.c_str(), json.length(), BB_STORE_RECORDS_OPDS_TOKENS, keepRecord<Entry>, &kept);
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
