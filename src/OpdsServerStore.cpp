#include "OpdsServerStore.h"

#include <Logging.h>
#include <ObfuscationUtils.h>
#include <StoreText.h>
#include <bembeyaz/store_opds_servers.h>

#include <algorithm>
#include <cstddef>
#include <cstring>

// The JSON is Rust's (bembeyaz_store::opds_servers); this file obfuscates the passwords and header
// values, passes the servers to Rust and takes them back. Adding, changing and removing servers
// stays here.
namespace {
static_assert(OpdsServer::MAX_CUSTOM_HEADERS == BB_STORE_HEADERS, "bb_store_opds_server's headers");
static_assert(sizeof(bb_store_header) == 2 * sizeof(bb_store_text) &&
                  offsetof(bb_store_header, value_obf) == sizeof(bb_store_text),
              "bb_store_header layout");
static_assert(sizeof(bb_store_opds_server) == 9 * sizeof(bb_store_text) + sizeof(size_t) &&
                  offsetof(bb_store_opds_server, password) == 4 * sizeof(bb_store_text) &&
                  offsetof(bb_store_opds_server, headers) == 5 * sizeof(bb_store_text) &&
                  offsetof(bb_store_opds_server, header_count) == 9 * sizeof(bb_store_text),
              "bb_store_opds_server layout");

// The servers read, and whether a legacy plain password was among them.
struct Read {
  std::vector<OpdsServer>& servers;
  bool needsResave;
};

void addRead(const bb_store_opds_server* stored, const size_t count, void* context) {
  auto* read = static_cast<Read*>(context);
  if (read->servers.empty()) read->servers.reserve(count);
  OpdsServer server;
  server.name = storeString(stored->name);
  server.url = storeString(stored->url);
  server.username = storeString(stored->username);
  server.password = readStoredPassword(stored->password_obf, stored->password, read->needsResave);
  for (size_t i = 0; i < stored->header_count && i < OpdsServer::MAX_CUSTOM_HEADERS; i++) {
    server.customHeaders[i].name = storeString(stored->headers[i].name);
    server.customHeaders[i].value =
        obfuscation::deobfuscateFromBase64(storeString(stored->headers[i].value_obf).c_str());
  }
  read->servers.push_back(std::move(server));
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
    out.password = bb_store_text{nullptr, 0};
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
  // A missing or invalid 'servers' key reads as an empty list; only a JSON parse error is fatal.
  std::vector<OpdsServer> read;
  Read context{read, false};
  const char* error = bb_store_opds_servers_read(json.c_str(), json.length(), MAX_SERVERS, addRead, &context);
  if (error) return error;
  servers = std::move(read);
  LOG_DBG("OPS", "Loaded %zu OPDS servers from file", servers.size());
  if (context.needsResave) {
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
