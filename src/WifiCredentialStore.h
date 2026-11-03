#pragma once
#include <PersistableStore.h>

#include <optional>
#include <string>
#include <vector>

struct WifiCredential {
  std::string ssid;
  std::string password;  // Plaintext in memory; obfuscated with hardware key on disk
};

struct WifiCredentialSummary {
  std::string ssid;
  bool hasPassword = false;
  bool isLastConnected = false;
};

/**
 * Singleton class for storing WiFi credentials on the SD card.
 * Passwords are XOR-obfuscated with the device's unique hardware MAC address
 * and base64-encoded before writing to JSON (not cryptographically secure,
 * but prevents casual reading and ties credentials to the specific device).
 */
class WifiCredentialStore : public PersistableStore<WifiCredentialStore> {
 private:
  std::vector<WifiCredential> credentials;
  std::string lastConnectedSsid;
  // Protects the in-memory strings independently of PersistableStore's file
  // serialization mutex. Readers only hold this briefly and never wait on SD
  // I/O; saveToFile() snapshots under this mutex from toJson().
  mutable std::mutex credentialMutex;

  // Rust's limits (BB_STORE_WIFI_MOST_*), checked in the .cpp.
  static constexpr size_t MAX_NETWORKS = 8;
  static constexpr size_t MAX_PASSWORD_LENGTH = 64;

  // Private constructor for singleton
  WifiCredentialStore() = default;

  friend class PersistableStore<WifiCredentialStore>;

 public:
  static const char* getFilePath() { return "/.crosspoint/wifi.json"; }
  // wifi.json's text, read, checked (each password's length and CRC-32) and written by Rust
  // (bembeyaz_store::wifi_credentials); the passwords' obfuscation stays here. See PersistableStore.
  bool toJsonText(String& json) const;
  const char* fromJsonText(const String& json);

  // Credential management
  bool addCredential(const std::string& ssid, const std::string& password);
  bool removeCredential(const std::string& ssid);
  std::optional<WifiCredential> findCredential(const std::string& ssid) const;
  std::optional<WifiCredential> getCredentialAt(size_t index) const;
  std::optional<std::string> getSsidAt(size_t index) const;
  size_t getCredentialCount() const;
  // Password-free snapshot for display/API consumers.
  std::vector<WifiCredentialSummary> getCredentialSummaries() const;

  // Check if a network is saved
  bool hasSavedCredential(const std::string& ssid) const;

  // Last connected network
  void setLastConnectedSsid(const std::string& ssid);
  std::string getLastConnectedSsid() const;
};

// Helper macro to access credentials store
#define WIFI_STORE WifiCredentialStore::getInstance()
