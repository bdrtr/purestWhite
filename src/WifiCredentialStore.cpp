#include "WifiCredentialStore.h"

#include <CredentialIntegrity.h>
#include <Logging.h>
#include <StoreText.h>
#include <bembeyaz/store_wifi_credentials.h>

#include <algorithm>
#include <cstddef>

// The JSON is Rust's (bembeyaz_store::wifi_credentials); this file obfuscates the passwords, checks
// what a file holds and decides which networks to keep, as the ArduinoJson version did.
namespace {
static_assert(sizeof(bb_store_number) == 8 && offsetof(bb_store_number, value) == 4, "bb_store_number layout");
static_assert(sizeof(bb_store_wifi_stored) == 3 * sizeof(bb_store_text) + 16 &&
                  offsetof(bb_store_wifi_stored, password_len) == 3 * sizeof(bb_store_text) &&
                  offsetof(bb_store_wifi_stored, password_crc32) == 3 * sizeof(bb_store_text) + 8,
              "bb_store_wifi_stored layout");
static_assert(sizeof(bb_store_wifi_saved) == 2 * sizeof(bb_store_text) + 16 &&
                  offsetof(bb_store_wifi_saved, password_len) == 2 * sizeof(bb_store_text) &&
                  offsetof(bb_store_wifi_saved, password_crc32) == 2 * sizeof(bb_store_text) + 8,
              "bb_store_wifi_saved layout");

// The networks read, the one connected last, the store's limits, and whether to save again.
struct Read {
  std::vector<WifiCredential>& credentials;
  std::string& lastConnectedSsid;
  size_t mostNetworks;
  size_t mostPasswordLength;
  bool needsResave;
};

void setLastConnected(const char* data, const size_t len, void* context) {
  static_cast<Read*>(context)->lastConnectedSsid.assign(data, len);
}

// Whether the password matches its stored length and CRC-32; an older file without them is kept,
// and needsResave set so it is saved again with them.
bool checkIntegrity(const WifiCredential& cred, const bb_store_wifi_stored& stored, bool& needsResave) {
  bool valid = true;
  if (stored.password_len.kind != BB_STORE_MISSING) {
    if (cred.password.size() != stored.password_len.value) {
      LOG_ERR("WCS", "Discarding corrupted password for %s (expected %zu bytes, decoded %zu)", cred.ssid.c_str(),
              static_cast<size_t>(stored.password_len.value), cred.password.size());
      valid = false;
    }
  } else {
    // Upgrade existing JSON after it has loaded successfully.
    needsResave = true;
  }
  if (stored.password_crc32.kind == BB_STORE_U32) {
    if (credential_integrity::crc32(cred.password) != stored.password_crc32.value) {
      LOG_ERR("WCS", "Discarding corrupted password for %s (checksum mismatch)", cred.ssid.c_str());
      valid = false;
    }
  } else if (stored.password_crc32.kind == BB_STORE_MISSING) {
    // password_crc32 was added after password_len; accept and upgrade files
    // written by older firmware.
    needsResave = true;
  } else {
    LOG_ERR("WCS", "Discarding corrupted password for %s (invalid checksum)", cred.ssid.c_str());
    valid = false;
  }
  return valid;
}

void addRead(const bb_store_wifi_stored* stored, const size_t count, void* context) {
  auto& read = *static_cast<Read*>(context);
  if (read.credentials.empty()) read.credentials.reserve(std::min(count, read.mostNetworks));
  if (read.credentials.size() >= read.mostNetworks) return;
  WifiCredential cred;
  cred.ssid = storeString(stored->ssid);
  // A length of another type, or one over mostPasswordLength, drops the network (and the file is
  // saved again without it).
  if (stored->password_len.kind == BB_STORE_OTHER) {
    LOG_ERR("WCS", "Discarding corrupted password for %s (invalid length)", cred.ssid.c_str());
    read.needsResave = true;
    return;
  }
  if (stored->password_len.kind == BB_STORE_U32 && stored->password_len.value > read.mostPasswordLength) {
    LOG_ERR("WCS", "Discarding oversized password for %s (%zu bytes)", cred.ssid.c_str(),
            static_cast<size_t>(stored->password_len.value));
    read.needsResave = true;
    return;
  }
  bool passwordValid = false;
  cred.password = readStoredPassword(stored->password_obf, stored->password, read.mostPasswordLength, read.needsResave,
                                     passwordValid);
  if (!passwordValid) {
    LOG_ERR("WCS", "Discarding oversized password for %s", cred.ssid.c_str());
    read.needsResave = true;
    return;
  }
  if (!checkIntegrity(cred, *stored, read.needsResave)) {
    read.needsResave = true;
    return;
  }
  read.credentials.push_back(std::move(cred));
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
    // XOR and base64 must round-trip to exactly this many bytes. Keeping the
    // original length lets the loader reject a decodable-but-corrupted value
    // instead of silently trying a different password.
    saved[i].password_len = cred.password.size();
    saved[i].password_crc32 = credential_integrity::crc32(cred.password);
  }
  if (!obfuscated.ok()) return false;
  StoreJsonSink sink{json, true};
  return bb_store_wifi_credentials_write(storeText(lastConnectedSsid), saved.data(), saved.size(), appendStoreJson,
                                         &sink) &&
         sink.ok;
}

const char* WifiCredentialStore::fromJsonText(const String& json) {
  std::lock_guard<std::mutex> lock(credentialMutex);
  // A missing or invalid 'credentials' key reads as an empty list; only a JSON parse error is fatal.
  std::vector<WifiCredential> read;
  std::string last;
  Read context{read, last, MAX_NETWORKS, MAX_PASSWORD_LENGTH, false};
  const char* error = bb_store_wifi_credentials_read(json.c_str(), json.length(), setLastConnected, addRead, &context);
  if (error) return error;
  credentials = std::move(read);
  lastConnectedSsid = std::move(last);
  LOG_DBG("WCS", "Loaded %zu WiFi credentials from file", credentials.size());
  if (context.needsResave) {
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
