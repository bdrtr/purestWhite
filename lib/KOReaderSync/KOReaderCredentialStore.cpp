#include "KOReaderCredentialStore.h"

#include <Logging.h>
#include <MD5Builder.h>
#include <ObfuscationUtils.h>
#include <StoreText.h>
#include <StoredPassword.h>
#include <bembeyaz/store_koreader_credentials.h>

#include <algorithm>
#include <cstddef>

namespace {
// Default sync server URL. crosspoint-sync speaks the full KOSync protocol, so
// pointing at any other kosync server (e.g. https://sync.koreader.rocks:443)
// still works via the custom server URL setting.
constexpr char DEFAULT_SERVER_URL[] = "https://sync.crosspointreader.com";

// Default before config version 2. Configs saved without a version stamp and an
// empty serverUrl were implicitly syncing here — they get pinned on upgrade.
constexpr char LEGACY_DEFAULT_SERVER_URL[] = "https://sync.koreader.rocks:443";

// Bumped when a change to defaults would alter behavior for existing configs.
constexpr uint8_t CONFIG_VERSION = 3;

static_assert(KOReaderCredentialStore::MAX_CUSTOM_HEADERS == BB_STORE_HEADERS, "bb_store_koreader_*'s headers");
static_assert(offsetof(bb_store_koreader_stored, headers) == 4 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_stored, header_count) == 8 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_stored, cfg_version) == 8 * sizeof(bb_store_text) + sizeof(size_t) &&
                  offsetof(bb_store_koreader_stored, sync_clippings) ==
                      8 * sizeof(bb_store_text) + sizeof(size_t) + 4 * sizeof(bb_store_number) + 1,
              "bb_store_koreader_stored layout");
static_assert(offsetof(bb_store_koreader_saved, headers) == 3 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_saved, header_count) == 7 * sizeof(bb_store_text) &&
                  offsetof(bb_store_koreader_saved, cfg_version) == 7 * sizeof(bb_store_text) + sizeof(size_t) &&
                  offsetof(bb_store_koreader_saved, sync_clippings) ==
                      7 * sizeof(bb_store_text) + sizeof(size_t) + 4 + 1,
              "bb_store_koreader_saved layout");

// What a koreader.json file holds, copied out of Rust's callback; the store decides on it after.
struct File {
  std::string username;
  std::string passwordObf;
  std::string password;
  std::string serverUrl;
  bb_store_number cfgVersion;
  bb_store_number serverType;
  bb_store_number matchMethod;
  bb_store_number syncBehavior;
  bool sendMetadata;
  bool syncClippings;
  size_t headerCount;
  std::array<HttpHeader, KOREADER_MAX_CUSTOM_HEADERS> headers;  // value as stored (obfuscated)
};

void copyFile(const bb_store_koreader_stored* stored, void* context) {
  File& file = *static_cast<File*>(context);
  file.username = storeString(stored->username);
  file.passwordObf = storeString(stored->password_obf);
  file.password = storeString(stored->password);
  file.serverUrl = storeString(stored->server_url);
  file.cfgVersion = stored->cfg_version;
  file.serverType = stored->server_type;
  file.matchMethod = stored->match_method;
  file.syncBehavior = stored->sync_behavior;
  file.sendMetadata = stored->send_metadata;
  file.syncClippings = stored->sync_clippings;
  file.headerCount = std::min(stored->header_count, file.headers.size());
  for (size_t i = 0; i < file.headerCount; i++) {
    file.headers[i] = {storeString(stored->headers[i].name), storeString(stored->headers[i].value_obf)};
  }
}

// fromJsonText's upgrades of an older config (v2: pin the old default server; v3: the server type
// from the URL), and its read of serverType otherwise.
void applyConfigVersion(KOReaderCredentialStore& store, const uint8_t cfgVersion,
                        const bb_store_number& serverTypeValue, bool& needsResave) {
  // The default server changed in config v2 (sync.koreader.rocks -> crosspoint-sync).
  // A pre-v2 config with credentials and no explicit URL was actively syncing
  // against the old default — pin that URL so the upgrade doesn't switch servers
  // out from under the user. Fresh setups get the new default.
  if (cfgVersion < 2) {
    if (store.getServerUrl().empty() && store.hasCredentials()) {
      LOG_DBG("KRS", "Pre-v2 config used the old default server; pinning %s", LEGACY_DEFAULT_SERVER_URL);
      store.setServerUrl(LEGACY_DEFAULT_SERVER_URL);
    }
    needsResave = true;
  }

  // v3 replaces hostname-based enhanced-progress detection with an explicit
  // protocol profile. Preserve previous behavior on upgrade: only the built-in
  // CrossPoint URL was enhanced, while every custom URL was strict KOSync.
  if (cfgVersion < CONFIG_VERSION || serverTypeValue.kind == BB_STORE_MISSING) {
    store.setServerType(store.getBaseUrl() == DEFAULT_SERVER_URL ? KOReaderServerType::CROSSPOINT
                                                                 : KOReaderServerType::KOSYNC);
    needsResave = true;
  } else {
    // as<uint8_t>(), whatever the value's type.
    const uint8_t type = serverTypeValue.byte;
    if (type <= static_cast<uint8_t>(KOReaderServerType::OTHER)) {
      store.setServerType(static_cast<KOReaderServerType>(type));
    } else {
      LOG_DBG("KRS", "Invalid serverType %u in JSON, resetting to KOSYNC", type);
      store.setServerType(KOReaderServerType::KOSYNC);
      needsResave = true;
    }
  }
}

// fromJsonText's read of syncBehavior.
void applySyncBehavior(KOReaderCredentialStore& store, const bb_store_number& behaviorValue, bool& needsResave) {
  const bool missingBehavior = behaviorValue.kind == BB_STORE_MISSING;
  uint8_t behavior = storeByteOr(behaviorValue, static_cast<uint8_t>(KOReaderSyncBehavior::ASK_EVERY_TIME));
  if (behavior <= static_cast<uint8_t>(KOReaderSyncBehavior::SMART)) {
    store.setSyncBehavior(static_cast<KOReaderSyncBehavior>(behavior));
    needsResave = needsResave || missingBehavior;
  } else {
    LOG_DBG("KRS", "Invalid syncBehavior %u in JSON, resetting to ASK_EVERY_TIME", behavior);
    store.setSyncBehavior(KOReaderSyncBehavior::ASK_EVERY_TIME);
    needsResave = true;
  }
}
}  // namespace

bool KOReaderCredentialStore::toJsonText(String& json) const {
  ObfuscatedTexts obfuscated;
  bb_store_koreader_saved saved{};
  saved.cfg_version = CONFIG_VERSION;
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
  File file{};
  const char* error = bb_store_koreader_credentials_read(json.c_str(), json.length(), copyFile, &file);
  if (error) return error;

  bool needsResave = false;
  std::string pass = obfuscation::readStoredPassword(file.passwordObf.c_str(), file.password.c_str(), needsResave);
  setCredentials(file.username, pass);
  setServerUrl(file.serverUrl);
  applyConfigVersion(*this, storeByteOr(file.cfgVersion, 1), file.serverType, needsResave);

  uint8_t method = storeByteOr(file.matchMethod, 0);
  if (method <= static_cast<uint8_t>(DocumentMatchMethod::BINARY)) {
    setMatchMethod(static_cast<DocumentMatchMethod>(method));
  } else {
    LOG_DBG("KRS", "Invalid matchMethod %u in JSON, resetting to FILENAME", method);
    setMatchMethod(DocumentMatchMethod::FILENAME);
  }
  setSendMetadata(file.sendMetadata);
  setSyncClippings(file.syncClippings);

  // Headers past the file's keep what they held.
  for (size_t i = 0; i < file.headerCount; i++) {
    customHeaders[i].name = file.headers[i].name;
    customHeaders[i].value = obfuscation::deobfuscateFromBase64(file.headers[i].value.c_str());
  }

  applySyncBehavior(*this, file.syncBehavior, needsResave);
  if (needsResave) {
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
  if (serverUrl.empty()) {
    url = DEFAULT_SERVER_URL;
  } else if (serverUrl.find("://") == std::string::npos) {
    // Normalize URL: add http:// if no protocol specified (local servers typically don't have SSL)
    url = "http://" + serverUrl;
  } else {
    url = serverUrl;
  }

  // Strip trailing slashes to avoid double-slash in API paths
  while (!url.empty() && url.back() == '/') {
    url.pop_back();
  }

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
