#include "KOReaderCredentialStore.h"

#include <Logging.h>
#include <MD5Builder.h>
#include <ObfuscationUtils.h>
#include <StoreText.h>
#include <bembeyaz/store_koreader_credentials.h>

#include <algorithm>
#include <cstddef>

namespace {
static_assert(KOReaderCredentialStore::MAX_CUSTOM_HEADERS == BB_STORE_HEADERS, "bb_store_koreader_*'s headers");
static_assert(static_cast<uint8_t>(KOReaderServerType::CROSSPOINT) == BB_STORE_KOREADER_SERVER_CROSSPOINT &&
                  static_cast<uint8_t>(KOReaderServerType::KOSYNC) == BB_STORE_KOREADER_SERVER_KOSYNC &&
                  static_cast<uint8_t>(KOReaderServerType::OTHER) == BB_STORE_KOREADER_SERVER_OTHER &&
                  static_cast<uint8_t>(DocumentMatchMethod::FILENAME) == BB_STORE_KOREADER_MATCH_FILENAME &&
                  static_cast<uint8_t>(DocumentMatchMethod::BINARY) == BB_STORE_KOREADER_MATCH_BINARY &&
                  static_cast<uint8_t>(KOReaderSyncBehavior::ASK_EVERY_TIME) == BB_STORE_KOREADER_SYNC_ASK_EVERY_TIME &&
                  static_cast<uint8_t>(KOReaderSyncBehavior::SMART) == BB_STORE_KOREADER_SYNC_SMART,
              "KOReader enums are bb_store_koreader_*'s values");
static_assert(offsetof(bb_store_koreader_settings, headers) == 3 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_settings, header_count) == 7 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_settings, server_type) == 7 * sizeof(bb_store_text) + sizeof(size_t) &&
                  offsetof(bb_store_koreader_settings, sync_behavior_reset) ==
                      7 * sizeof(bb_store_text) + sizeof(size_t) + 9,
              "bb_store_koreader_settings layout");
static_assert(offsetof(bb_store_koreader_saved, headers) == 3 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_saved, header_count) == 7 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_saved, server_type) == 7 * sizeof(bb_store_text) + sizeof(size_t) &&
                  offsetof(bb_store_koreader_saved, sync_clippings) == 7 * sizeof(bb_store_text) + sizeof(size_t) + 4,
              "bb_store_koreader_saved layout");

// fromJsonText's receiver of what Rust made of the file.
struct Settling {
  KOReaderCredentialStore& store;
  bool resave;
};

// What Rust's checks changed, logged as the C++ store did.
void logChecks(const bb_store_koreader_settings& s) {
  if (s.pinned_legacy_url) {
    LOG_DBG("KRS", "Pre-v2 config used the old default server; pinning %s", storeString(s.server_url).c_str());
  }
  if (s.server_type_reset) {
    LOG_DBG("KRS", "Invalid serverType %u in JSON, resetting to KOSYNC", s.server_type_reset);
  }
  if (s.match_method_reset) {
    LOG_DBG("KRS", "Invalid matchMethod %u in JSON, resetting to FILENAME", s.match_method_reset);
  }
  if (s.sync_behavior_reset) {
    LOG_DBG("KRS", "Invalid syncBehavior %u in JSON, resetting to ASK_EVERY_TIME", s.sync_behavior_reset);
  }
}

void applySettings(const bb_store_koreader_settings* s, void* context) {
  Settling& settling = *static_cast<Settling*>(context);
  KOReaderCredentialStore& store = settling.store;
  logChecks(*s);
  store.setCredentials(storeString(s->username), storeString(s->password));
  store.setServerUrl(storeString(s->server_url));
  store.setServerType(static_cast<KOReaderServerType>(s->server_type));
  store.setMatchMethod(static_cast<DocumentMatchMethod>(s->match_method));
  store.setSendMetadata(s->send_metadata);
  store.setSyncClippings(s->sync_clippings);
  // Headers past the file's keep what they held.
  for (size_t i = 0; i < std::min(s->header_count, KOReaderCredentialStore::MAX_CUSTOM_HEADERS); i++) {
    store.setCustomHeader(i, storeString(s->headers[i].name), storeString(s->headers[i].value));
  }
  store.setSyncBehavior(static_cast<KOReaderSyncBehavior>(s->sync_behavior));
  settling.resave = s->resave;
}
}  // namespace

bool KOReaderCredentialStore::toJsonText(String& json) const {
  ObfuscatedTexts obfuscated;
  bb_store_koreader_saved saved{};
  saved.username = storeText(getUsername());
  saved.password_obf = obfuscated.add(getPassword());
  saved.server_url = storeText(getServerUrl());
  saved.server_type = static_cast<uint8_t>(getServerType());
  saved.match_method = static_cast<uint8_t>(getMatchMethod());
  saved.send_metadata = getSendMetadata();
  saved.sync_clippings = getSyncClippings();
  saved.sync_behavior = static_cast<uint8_t>(getSyncBehavior());
  for (size_t i = 0; i < MAX_CUSTOM_HEADERS; i++) {
    saved.headers[i].name = storeText(customHeaders[i].name);
    saved.headers[i].value_obf = obfuscated.add(customHeaders[i].value);
  }
  saved.header_count = MAX_CUSTOM_HEADERS;
  if (!obfuscated.ok()) return false;
  StoreJsonSink sink{json, true};
  return bb_store_koreader_credentials_write(&saved, appendStoreJson, &sink) && sink.ok;
}

const char* KOReaderCredentialStore::fromJsonText(const String& json) {
  Settling settling{*this, false};
  const char* error =
      bb_store_koreader_credentials_settle(json.c_str(), json.length(), decodeStoredSecret, applySettings, &settling);
  if (error) return error;
  if (settling.resave) {
    LOG_DBG("KRS", "Resaving KOReader credentials to update format");
    requestResave();
  }
  return nullptr;
}

void KOReaderCredentialStore::setCredentials(const std::string& user, const std::string& pass) {
  username = user;
  password = pass;
  LOG_DBG("KRS", "Set credentials for user: %s", user.c_str());
}

std::string KOReaderCredentialStore::getMd5Password() const {
  if (password.empty()) {
    return "";
  }

  // Calculate MD5 hash of password using ESP32's MD5Builder
  MD5Builder md5;
  md5.begin();
  md5.add(password.c_str());
  md5.calculate();

  return md5.toString().c_str();
}

bool KOReaderCredentialStore::hasCredentials() const { return !username.empty() && !password.empty(); }

void KOReaderCredentialStore::clearCredentials() {
  username.clear();
  password.clear();
  saveToFile();
  LOG_DBG("KRS", "Cleared KOReader credentials");
}

void KOReaderCredentialStore::setServerUrl(const std::string& url) {
  serverUrl = url;
  LOG_DBG("KRS", "Set server URL: %s", url.empty() ? "(default)" : url.c_str());
}

std::string KOReaderCredentialStore::getBaseUrl() const {
  std::string url;
  bb_store_koreader_base_url(serverUrl.data(), serverUrl.size(), appendStoreString, &url);
  return url;
}

void KOReaderCredentialStore::setServerType(KOReaderServerType type) {
  if (static_cast<uint8_t>(type) > static_cast<uint8_t>(KOReaderServerType::OTHER)) {
    type = KOReaderServerType::KOSYNC;
  }
  serverType = type;
  LOG_DBG("KRS", "Set server type: %u", static_cast<unsigned>(type));
}

void KOReaderCredentialStore::setMatchMethod(DocumentMatchMethod method) {
  matchMethod = method;
  LOG_DBG("KRS", "Set match method: %s", method == DocumentMatchMethod::FILENAME ? "Filename" : "Binary");
}

void KOReaderCredentialStore::setSendMetadata(bool enabled) {
  sendMetadata = enabled;
  LOG_DBG("KRS", "Set send metadata: %s", enabled ? "true" : "false");
}

void KOReaderCredentialStore::setSyncBehavior(KOReaderSyncBehavior behavior) {
  if (static_cast<uint8_t>(behavior) > static_cast<uint8_t>(KOReaderSyncBehavior::SMART)) {
    behavior = KOReaderSyncBehavior::ASK_EVERY_TIME;
  }
  syncBehavior = behavior;
  LOG_DBG("KRS", "Set sync behavior: %s", behavior == KOReaderSyncBehavior::SMART ? "Smart" : "Ask");
}

void KOReaderCredentialStore::setCustomHeader(size_t index, const std::string& name, const std::string& value) {
  if (index >= MAX_CUSTOM_HEADERS) return;
  customHeaders[index].name = name;
  customHeaders[index].value = value;
  LOG_DBG("KRS", "Set custom header %zu: %s", index, name.empty() ? "(cleared)" : name.c_str());
}
