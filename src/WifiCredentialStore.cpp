#include "WifiCredentialStore.h"

#include <Logging.h>
#include <StoreText.h>
#include <bembeyaz/store_wifi_credentials.h>

#include <algorithm>
#include <cstddef>

// The JSON is Rust's (bembeyaz_store::wifi_credentials), and so are the checks of what a file holds
// and which networks to keep (wifi_credentials::keep); this file obfuscates the passwords and logs
// the networks dropped.
namespace {
static_assert(sizeof(bb_store_wifi_saved) == 3 * sizeof(bb_store_text) &&
                  offsetof(bb_store_wifi_saved, password) == 2 * sizeof(bb_store_text),
              "bb_store_wifi_saved layout");
static_assert(BB_STORE_WIFI_INVALID_LENGTH == 0 && BB_STORE_WIFI_OVERSIZED_LENGTH == 1 &&
                  BB_STORE_WIFI_OVERSIZED_PASSWORD == 2 && BB_STORE_WIFI_CORRUPTED == 3 &&
                  BB_STORE_WIFI_CHECKSUM_FINE == 0 && BB_STORE_WIFI_CHECKSUM_MISMATCH == 1 &&
                  BB_STORE_WIFI_CHECKSUM_INVALID == 2,
              "bb_store_wifi_dropped's codes are Rust's");
static_assert(sizeof(bb_store_wifi_kept) == 2 * sizeof(bb_store_text), "bb_store_wifi_kept layout");
static_assert(offsetof(bb_store_wifi_dropped, length) == sizeof(bb_store_text) + 4 &&
                  offsetof(bb_store_wifi_dropped, decoded) == sizeof(bb_store_text) + 8 &&
                  sizeof(bb_store_wifi_dropped) == sizeof(bb_store_text) + 8 + sizeof(size_t),
              "bb_store_wifi_dropped layout");
static_assert(sizeof(bb_store_wifi_keep_calls) == 4 * sizeof(void*), "bb_store_wifi_keep_calls layout");

// The networks kept and the one connected last, as Rust hands them over.
struct Kept {
  std::vector<WifiCredential>& credentials;
  std::string& lastConnectedSsid;
};

void setLastConnected(const char* data, const size_t len, void* context) {
  static_cast<Kept*>(context)->lastConnectedSsid.assign(data, len);
}

void addKept(const bb_store_wifi_kept* kept, void* context) {
  static_cast<Kept*>(context)->credentials.push_back({storeString(kept->ssid), storeString(kept->password)});
}

// Why a network was dropped, logged as the store did when it decided in C++.
void logDropped(const bb_store_wifi_dropped* dropped, void*) {
  const std::string ssid = storeString(dropped->ssid);
  switch (dropped->why) {
    case BB_STORE_WIFI_INVALID_LENGTH:
      LOG_ERR("WCS", "Discarding corrupted password for %s (invalid length)", ssid.c_str());
      return;
    case BB_STORE_WIFI_OVERSIZED_LENGTH:
      LOG_ERR("WCS", "Discarding oversized password for %s (%zu bytes)", ssid.c_str(),
              static_cast<size_t>(dropped->length));
      return;
    case BB_STORE_WIFI_OVERSIZED_PASSWORD:
      LOG_ERR("WCS", "Discarding oversized password for %s", ssid.c_str());
      return;
    default:
      break;
  }
  if (dropped->length_mismatch) {
    LOG_ERR("WCS", "Discarding corrupted password for %s (expected %zu bytes, decoded %zu)", ssid.c_str(),
            static_cast<size_t>(dropped->length), dropped->decoded);
  }
  if (dropped->checksum == BB_STORE_WIFI_CHECKSUM_MISMATCH) {
    LOG_ERR("WCS", "Discarding corrupted password for %s (checksum mismatch)", ssid.c_str());
  } else if (dropped->checksum == BB_STORE_WIFI_CHECKSUM_INVALID) {
    LOG_ERR("WCS", "Discarding corrupted password for %s (invalid checksum)", ssid.c_str());
  }
}
}  // namespace

bool WifiCredentialStore::toJsonText(String& json) const {
  std::lock_guard<std::mutex> lock(credentialMutex);
  ObfuscatedTexts obfuscated;
  std::vector<bb_store_wifi_saved> saved(credentials.size());
  for (size_t i = 0; i < credentials.size(); i++) {
    const WifiCredential& cred = credentials[i];
    saved[i].ssid = storeText(cred.ssid);
    saved[i].password_obf = obfuscated.add(cred.password);
    // Rust saves its length and CRC-32 with it, so a decodable but corrupted value is refused on
    // load instead of trying a different password.
    saved[i].password = storeText(cred.password);
  }
  if (!obfuscated.ok()) return false;
  StoreJsonSink sink{json, true};
  return bb_store_wifi_credentials_write(storeText(lastConnectedSsid), saved.data(), saved.size(), appendStoreJson,
                                         &sink) &&
         sink.ok;
}

const char* WifiCredentialStore::fromJsonText(const String& json) {
  static_assert(MAX_NETWORKS == BB_STORE_WIFI_MOST_NETWORKS && MAX_PASSWORD_LENGTH == BB_STORE_WIFI_MOST_PASSWORD,
                "the store's limits are Rust's");
  std::lock_guard<std::mutex> lock(credentialMutex);
  // A missing or invalid 'credentials' key reads as an empty list; only a JSON parse error is fatal.
  std::vector<WifiCredential> read;
  std::string last;
  read.reserve(MAX_NETWORKS);
  Kept kept{read, last};
  const bb_store_wifi_keep_calls calls{decodeStoredSecret, setLastConnected, addKept, logDropped};
  bool needsResave = false;
  const char* error = bb_store_wifi_credentials_keep(json.c_str(), json.length(), &calls, &kept, &needsResave);
  if (error) return error;
  credentials = std::move(read);
  lastConnectedSsid = std::move(last);
  LOG_DBG("WCS", "Loaded %zu WiFi credentials from file", credentials.size());
  if (needsResave) {
    LOG_DBG("WCS", "Resaving JSON with obfuscated passwords");
    requestResave();
  }
  return nullptr;
}

bool WifiCredentialStore::addCredential(const std::string& ssid, const std::string& password) {
  {
    std::lock_guard<std::mutex> lock(credentialMutex);

    // Check if this SSID already exists and update it
    const auto cred = find_if(credentials.begin(), credentials.end(),
                              [&ssid](const WifiCredential& cred) { return cred.ssid == ssid; });
    if (cred != credentials.end()) {
      cred->password = password;
      LOG_DBG("WCS", "Updated credentials for: %s", ssid.c_str());
    } else {
      // Check if we've reached the limit
      if (credentials.size() >= MAX_NETWORKS) {
        LOG_DBG("WCS", "Cannot add more networks, limit of %zu reached", MAX_NETWORKS);
        return false;
      }

      // Add new credential
      credentials.push_back({ssid, password});
      LOG_DBG("WCS", "Added credentials for: %s", ssid.c_str());
    }
  }
  return saveToFile();
}

bool WifiCredentialStore::removeCredential(const std::string& ssid) {
  {
    std::lock_guard<std::mutex> lock(credentialMutex);
    const auto cred = find_if(credentials.begin(), credentials.end(),
                              [&ssid](const WifiCredential& cred) { return cred.ssid == ssid; });
    if (cred == credentials.end()) {
      return false;  // Not found
    }
    credentials.erase(cred);
    LOG_DBG("WCS", "Removed credentials for: %s", ssid.c_str());
    if (ssid == lastConnectedSsid) lastConnectedSsid.clear();
  }
  return saveToFile();
}

std::optional<WifiCredential> WifiCredentialStore::findCredential(const std::string& ssid) const {
  std::lock_guard<std::mutex> lock(credentialMutex);

  const auto cred = find_if(credentials.begin(), credentials.end(),
                            [&ssid](const WifiCredential& cred) { return cred.ssid == ssid; });

  if (cred != credentials.end()) {
    return *cred;
  }

  return std::nullopt;
}

std::optional<WifiCredential> WifiCredentialStore::getCredentialAt(const size_t index) const {
  std::lock_guard<std::mutex> lock(credentialMutex);
  if (index >= credentials.size()) return std::nullopt;
  return credentials[index];
}

std::optional<std::string> WifiCredentialStore::getSsidAt(const size_t index) const {
  std::lock_guard<std::mutex> lock(credentialMutex);
  if (index >= credentials.size()) return std::nullopt;
  return credentials[index].ssid;
}

size_t WifiCredentialStore::getCredentialCount() const {
  std::lock_guard<std::mutex> lock(credentialMutex);
  return credentials.size();
}

std::vector<WifiCredentialSummary> WifiCredentialStore::getCredentialSummaries() const {
  std::lock_guard<std::mutex> lock(credentialMutex);
  std::vector<WifiCredentialSummary> summaries;
  summaries.reserve(credentials.size());
  for (const auto& credential : credentials) {
    summaries.push_back({credential.ssid, !credential.password.empty(), credential.ssid == lastConnectedSsid});
  }
  return summaries;
}

bool WifiCredentialStore::hasSavedCredential(const std::string& ssid) const {
  std::lock_guard<std::mutex> lock(credentialMutex);
  return find_if(credentials.begin(), credentials.end(),
                 [&ssid](const WifiCredential& cred) { return cred.ssid == ssid; }) != credentials.end();
}

void WifiCredentialStore::setLastConnectedSsid(const std::string& ssid) {
  {
    std::lock_guard<std::mutex> lock(credentialMutex);
    if (lastConnectedSsid == ssid) return;
    lastConnectedSsid = ssid;
  }
  saveToFile();
}

std::string WifiCredentialStore::getLastConnectedSsid() const {
  std::lock_guard<std::mutex> lock(credentialMutex);
  return lastConnectedSsid;
}
