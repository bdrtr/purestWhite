#include "OpdsServerStore.h"

#include <Logging.h>
#include <StoreText.h>
#include <bembeyaz/store_opds_servers.h>

#include <algorithm>
#include <cstddef>
#include <cstring>

// The JSON is Rust's (bembeyaz_store::opds_servers), and so is what a file makes (the passwords and
// header values decoded, a legacy plain password resaved, at most MAX_SERVERS); this file
// obfuscates the passwords and header values for saving. Adding, changing and removing servers
// stays here.
namespace {
static_assert(OpdsServer::MAX_CUSTOM_HEADERS == BB_STORE_HEADERS, "bb_store_opds_*'s headers");
static_assert(sizeof(bb_store_header) == 2 * sizeof(bb_store_text) &&
                  offsetof(bb_store_header, value_obf) == sizeof(bb_store_text),
              "bb_store_header layout");
static_assert(sizeof(bb_store_plain_header) == 2 * sizeof(bb_store_text) &&
                  offsetof(bb_store_plain_header, value) == sizeof(bb_store_text),
              "bb_store_plain_header layout");
static_assert(sizeof(bb_store_opds_server) == 8 * sizeof(bb_store_text) + sizeof(size_t) &&
                  offsetof(bb_store_opds_server, headers) == 4 * sizeof(bb_store_text) &&
                  offsetof(bb_store_opds_server, header_count) == 8 * sizeof(bb_store_text),
              "bb_store_opds_server layout");
static_assert(sizeof(bb_store_opds_loaded) == 8 * sizeof(bb_store_text) + sizeof(size_t) &&
                  offsetof(bb_store_opds_loaded, headers) == 4 * sizeof(bb_store_text) &&
                  offsetof(bb_store_opds_loaded, header_count) == 8 * sizeof(bb_store_text),
              "bb_store_opds_loaded layout");

void addLoaded(const bb_store_opds_loaded* loaded, const size_t count, void* context) {
  auto& servers = *static_cast<std::vector<OpdsServer>*>(context);
  if (servers.empty()) servers.reserve(count);
  OpdsServer server;
  server.name = storeString(loaded->name);
  server.url = storeString(loaded->url);
  server.username = storeString(loaded->username);
  server.password = storeString(loaded->password);
  for (size_t i = 0; i < loaded->header_count && i < OpdsServer::MAX_CUSTOM_HEADERS; i++) {
    server.customHeaders[i] = {storeString(loaded->headers[i].name), storeString(loaded->headers[i].value)};
  }
  servers.push_back(std::move(server));
}
}  // namespace

bool OpdsServerStore::toJsonText(String& json) const {
  ObfuscatedTexts obfuscated;
  std::vector<bb_store_opds_server> stored(servers.size());
  for (size_t i = 0; i < servers.size(); i++) {
    const OpdsServer& server = servers[i];
    bb_store_opds_server& out = stored[i];
    out.name = storeText(server.name);
    out.url = storeText(server.url);
    out.username = storeText(server.username);
    out.password_obf = obfuscated.add(server.password);
    for (size_t h = 0; h < OpdsServer::MAX_CUSTOM_HEADERS; h++) {
      out.headers[h].name = storeText(server.customHeaders[h].name);
      out.headers[h].value_obf = obfuscated.add(server.customHeaders[h].value);
    }
    out.header_count = OpdsServer::MAX_CUSTOM_HEADERS;
  }
  if (!obfuscated.ok()) return false;
  StoreJsonSink sink{json, true};
  return bb_store_opds_servers_write(stored.data(), stored.size(), appendStoreJson, &sink) && sink.ok;
}

const char* OpdsServerStore::fromJsonText(const String& json) {
  static_assert(MAX_SERVERS == BB_STORE_OPDS_MOST_SERVERS, "the store's limit is Rust's");
  // A missing or invalid 'servers' key reads as an empty list; a JSON parse error, or memory running
  // out, keeps the current servers.
  std::vector<OpdsServer> read;
  bool needsResave = false;
  const char* error =
      bb_store_opds_servers_load(json.c_str(), json.length(), decodeStoredSecret, addLoaded, &read, &needsResave);
  if (error) return error;
  servers = std::move(read);
  LOG_DBG("OPS", "Loaded %zu OPDS servers from file", servers.size());
  if (needsResave) {
    LOG_DBG("OPS", "Resaving JSON with obfuscated passwords");
    requestResave();
  }
  return nullptr;
}

bool OpdsServerStore::addServer(const OpdsServer& server) {
  if (servers.size() >= MAX_SERVERS) {
    LOG_DBG("OPS", "Cannot add more servers, limit of %zu reached", MAX_SERVERS);
    return false;
  }

  servers.push_back(server);
  LOG_DBG("OPS", "Added server: %s", server.name.c_str());
  return saveToFile();
}

bool OpdsServerStore::updateServer(size_t index, const OpdsServer& server) {
  if (index >= servers.size()) {
    return false;
  }

  servers[index] = server;
  LOG_DBG("OPS", "Updated server: %s", server.name.c_str());
  return saveToFile();
}

bool OpdsServerStore::removeServer(size_t index) {
  if (index >= servers.size()) {
    return false;
  }

  LOG_DBG("OPS", "Removed server: %s", servers[index].name.c_str());
  servers.erase(servers.begin() + static_cast<ptrdiff_t>(index));
  return saveToFile();
}

const OpdsServer* OpdsServerStore::getServer(size_t index) const {
  if (index >= servers.size()) {
    return nullptr;
  }
  return &servers[index];
}

std::vector<HttpDownloader::Header> OpdsServer::activeCustomHeaders() const {
  std::vector<HttpDownloader::Header> active;
  active.reserve(MAX_CUSTOM_HEADERS);
  for (const auto& header : customHeaders) {
    if (!header.name.empty()) active.emplace_back(header.name, header.value);
  }
  return active;
}
