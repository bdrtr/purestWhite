#pragma once
#include <PersistableStore.h>

#include <string>
#include <vector>

/**
 * OAuth token state for one OPDS server, keyed by server URL.
 * accessToken/refreshToken are bearer credentials, stored XOR-obfuscated with
 * the device key (same protection as the server password). refreshUrl is the
 * token endpoint discovered from the authentication document; it lets an
 * expired access token be refreshed without a full re-login.
 */
struct OpdsTokens {
  std::string accessToken;
  std::string refreshToken;
  std::string refreshUrl;

  bool empty() const { return accessToken.empty(); }
};

/**
 * Persists per-server OAuth tokens on SD (/.crosspoint/opds_tokens.json) so the
 * device does not re-authenticate every session, and can refresh an expired
 * access token using its refresh token.
 */
class OpdsTokenStore : public PersistableStore<OpdsTokenStore> {
 public:
  // One entry as kept; plain data.
  struct Entry {
    std::string url;
    OpdsTokens tokens;
  };

 private:
  std::vector<Entry> entries;
  static constexpr size_t MAX_SERVERS = 8;

  OpdsTokenStore() = default;
  friend class PersistableStore<OpdsTokenStore>;

 public:
  static const char* getFilePath() { return "/.crosspoint/opds_tokens.json"; }
  // opds_tokens.json's text, read and written by Rust (bembeyaz_store::records); the tokens'
  // obfuscation, and which entries are kept, stay here. See PersistableStore.
  bool toJsonText(String& json) const;
  const char* fromJsonText(const String& json);

  // Returns the stored tokens for a server URL (empty when none).
  OpdsTokens get(const std::string& url) const;
  // Upserts the tokens for a server URL. A no-op (no SD write) when unchanged;
  // empty tokens remove the entry. Returns true on success.
  bool put(const std::string& url, const OpdsTokens& tokens);
};

#define OPDS_TOKENS OpdsTokenStore::getInstance()
