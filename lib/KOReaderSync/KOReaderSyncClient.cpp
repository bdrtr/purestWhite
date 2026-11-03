#include "KOReaderSyncClient.h"

#include <HalMemory.h>
#include <I18n.h>
#include <Logging.h>
#include <SecureHttpClient.h>
#include <base64.h>

#include <string>

#include "KOReaderCredentialStore.h"
#include "KOReaderSyncMessages.h"

int KOReaderSyncClient::lastHttpCode = 0;

namespace {
// wolfSSL uses the default allocator, which can use PSRAM on supported builds.
// Keep a free-space floor and room for a full TLS record when the server does
// not negotiate our smaller record limit. These are preflight margins, not a
// guarantee that a handshake will fit.
constexpr uint32_t MIN_FREE_FOR_TLS = 35000;
constexpr uint32_t MIN_BLOCK_FOR_TLS = 20000;

// Apply the shared KOSync auth headers after begin(). x-auth-* is the native
// KOSync scheme; Basic auth is added for Calibre-Web-Automated compatibility.
void applyAuthHeaders(freeink::SecureHttpClient& http) {
  http.addHeader("Accept", "application/vnd.koreader.v1+json");
  http.addHeader("x-auth-user", KOREADER_STORE.getUsername());
  http.addHeader("x-auth-key", KOREADER_STORE.getMd5Password());
  const std::string credentials = KOREADER_STORE.getUsername() + ":" + KOREADER_STORE.getPassword();
  const String encoded = base64::encode(credentials.c_str());
  http.addHeader("Authorization", std::string("Basic ") + encoded.c_str());
}

// Caller-configured headers (e.g. a Cloudflare Access service token) applied
// on top of the KOSync auth scheme, including on createUser() which has no
// KOSync credentials yet.
void applyCustomHeaders(freeink::SecureHttpClient& http) {
  for (const auto& header : KOREADER_STORE.getCustomHeaders()) {
    if (!header.name.empty()) http.addHeader(header.name, header.value);
  }
}

// The error for a request that got no response.
KOReaderSyncClient::Error transportError(const freeink::SecureHttpClient& http) {
  return http.certificateRejected() ? KOReaderSyncClient::CERTIFICATE_REJECTED : KOReaderSyncClient::NETWORK_ERROR;
}

// True when free heap is too low to risk a TLS handshake.
bool insufficientHeap() {
  const auto heap = HalMemory::getDefaultHeap();
  if (heap.freeBytes < MIN_FREE_FOR_TLS || heap.largestBlockBytes < MIN_BLOCK_FOR_TLS) {
    LOG_ERR("KOSync",
            "Insufficient allocatable heap for TLS handshake: %zu bytes free (need %u), %zu max alloc (need %u)",
            heap.freeBytes, MIN_FREE_FOR_TLS, heap.largestBlockBytes, MIN_BLOCK_FOR_TLS);
    return true;
  }
  return false;
}
}  // namespace

KOReaderSyncClient::Error KOReaderSyncClient::authenticate() {
  lastHttpCode = 0;
  if (!KOREADER_STORE.hasCredentials()) {
    LOG_DBG("KOSync", "No credentials configured");
    return NO_CREDENTIALS;
  }

  const std::string url = KOREADER_STORE.getBaseUrl() + "/users/auth";
  LOG_DBG("KOSync", "Authenticating: %s (heap: %u)", url.c_str(), (unsigned)ESP.getFreeHeap());
  if (insufficientHeap()) return LOW_MEMORY;

  freeink::SecureHttpClient http;
  if (!http.begin(url)) {
    LOG_ERR("KOSync", "Bad URL: %s", url.c_str());
    return NETWORK_ERROR;
  }
  applyCustomHeaders(http);
  applyAuthHeaders(http);
  const int httpCode = http.GET();
  http.end();
  lastHttpCode = httpCode;

  LOG_DBG("KOSync", "Auth response: %d", httpCode);

  if (httpCode <= 0) return transportError(http);
  // Any 2xx is success. The reference kosync server answers 200, but
  // KOSync-compatible implementations differ (BookLore/grimmory is a Spring
  // service and uses the idiomatic codes) — see issue #2876.
  if (httpCode >= 200 && httpCode < 300) return OK;
  if (httpCode == 401) return AUTH_FAILED;
  return SERVER_ERROR;
}

KOReaderSyncClient::Error KOReaderSyncClient::createUser() {
  lastHttpCode = 0;
  if (!KOREADER_STORE.hasCredentials()) {
    LOG_DBG("KOSync", "No credentials configured");
    return NO_CREDENTIALS;
  }

  const std::string url = KOREADER_STORE.getBaseUrl() + "/users/create";
  LOG_DBG("KOSync", "Creating account: %s (heap: %u)", url.c_str(), (unsigned)ESP.getFreeHeap());
  if (insufficientHeap()) return LOW_MEMORY;

  const std::string body = kosync::createUserBody(KOREADER_STORE.getUsername(), KOREADER_STORE.getMd5Password());

  freeink::SecureHttpClient http;
  if (!http.begin(url)) {
    LOG_ERR("KOSync", "Bad URL: %s", url.c_str());
    return NETWORK_ERROR;
  }
  applyCustomHeaders(http);
  http.addHeader("Accept", "application/vnd.koreader.v1+json");
  http.addHeader("Content-Type", "application/json");
  const int httpCode = http.sendRequest("POST", body);
  http.end();
  lastHttpCode = httpCode;

  LOG_DBG("KOSync", "Create user response: %d", httpCode);

  if (httpCode <= 0) return transportError(http);
  if (httpCode >= 200 && httpCode < 300) return OK;  // 2xx: created (see #2876)
  if (httpCode == 402) return USER_EXISTS;
  return SERVER_ERROR;
}

KOReaderSyncClient::Error KOReaderSyncClient::getProgress(const std::string& documentHash,
                                                          KOReaderProgress& outProgress,
                                                          freeink::SecureHttpClient* connection) {
  lastHttpCode = 0;
  if (!KOREADER_STORE.hasCredentials()) {
    LOG_DBG("KOSync", "No credentials configured");
    return NO_CREDENTIALS;
  }

  const std::string url =
      KOREADER_STORE.getBaseUrl() + "/syncs/progress/" + documentHash + "?position_kinds=locator,percentage";
  LOG_DBG("KOSync", "Getting progress: %s (heap: %u)", url.c_str(), (unsigned)ESP.getFreeHeap());
  if (insufficientHeap()) return LOW_MEMORY;

  freeink::SecureHttpClient own;
  freeink::SecureHttpClient& http = connection ? *connection : own;
  // A caller's connection stays open for its next request.
  const auto release = [&] {
    if (!connection) http.end();
  };
  if (!http.begin(url)) {
    LOG_ERR("KOSync", "Bad URL: %s", url.c_str());
    return NETWORK_ERROR;
  }
  applyCustomHeaders(http);
  applyAuthHeaders(http);
  const int httpCode = http.GET();
  lastHttpCode = httpCode;

  LOG_DBG("KOSync", "Get progress response: %d", httpCode);

  if (httpCode <= 0) {
    release();
    return transportError(http);
  }

  // 204 = success with no stored progress for this document (Spring-style
  // KOSync implementations; the reference server answers 200 with an empty
  // object instead). Map it to the same graceful no-remote-progress path as
  // 404 rather than falling through to SERVER_ERROR — see issue #2876.
  if (httpCode == 204) {
    release();
    return NOT_FOUND;
  }

  if (httpCode >= 200 && httpCode < 300) {
    const std::string answer = http.getString();
    release();
    const char* error =
        kosync::parseProgress(answer.c_str(), documentHash, KOREADER_STORE.supportsRichProgress(), outProgress);

    if (error) {
      LOG_ERR("KOSync", "JSON parse failed: %s", error);
      return JSON_ERROR;
    }

    if (outProgress.position.has_value()) {
      const KOReaderRichPosition& rich = *outProgress.position;
      LOG_DBG("KOSync", "Got rich position: spine=%u page=%u/%u para=%u", rich.spineIndex, rich.pageNumber,
              rich.totalPages, rich.paragraphIndex.value_or(0));
    }

    LOG_DBG("KOSync", "Got progress: %.2f%% at %s", outProgress.percentage * 100, outProgress.progress.c_str());
    return OK;
  }

  release();
  if (httpCode == 401) return AUTH_FAILED;
  if (httpCode == 404) return NOT_FOUND;
  return SERVER_ERROR;
}

KOReaderSyncClient::Error KOReaderSyncClient::updateProgress(const KOReaderProgress& progress,
                                                             freeink::SecureHttpClient* connection) {
  lastHttpCode = 0;
  if (!KOREADER_STORE.hasCredentials()) {
    LOG_DBG("KOSync", "No credentials configured");
    return NO_CREDENTIALS;
  }

  const std::string url = KOREADER_STORE.getBaseUrl() + "/syncs/progress";
  LOG_DBG("KOSync", "Updating progress: %s (heap: %u)", url.c_str(), (unsigned)ESP.getFreeHeap());
  if (insufficientHeap()) return LOW_MEMORY;

  const std::string body =
      kosync::progressBody(progress, KOREADER_STORE.supportsExtendedMetadata(), KOREADER_STORE.supportsRichProgress());

  LOG_DBG("KOSync", "Request body: %s", body.c_str());

  freeink::SecureHttpClient own;
  freeink::SecureHttpClient& http = connection ? *connection : own;
  // A caller's connection stays open for its next request.
  const auto release = [&] {
    if (!connection) http.end();
  };
  if (!http.begin(url)) {
    LOG_ERR("KOSync", "Bad URL: %s", url.c_str());
    return NETWORK_ERROR;
  }
  applyCustomHeaders(http);
  applyAuthHeaders(http);
  http.addHeader("Content-Type", "application/json");
  const int httpCode = http.sendRequest("PUT", body);
  release();
  lastHttpCode = httpCode;

  LOG_DBG("KOSync", "Update progress response: %d", httpCode);

  if (httpCode <= 0) return transportError(http);
  // Any 2xx accepts the progress. The reference kosync server answers 200,
  // but Spring-based KOSync implementations (BookLore/grimmory) answer a PUT
  // with the idiomatic 201/204, which used to land in SERVER_ERROR and made
  // every sync against them fail after a successful pull — issue #2876.
  if (httpCode >= 200 && httpCode < 300) return OK;
  if (httpCode == 401) return AUTH_FAILED;
  return SERVER_ERROR;
}

const char* KOReaderSyncClient::errorString(Error error) {
  switch (error) {
    case OK:
      return "Success";
    case NO_CREDENTIALS:
      return "No credentials configured";
    case NETWORK_ERROR:
      return "Network error";
    case AUTH_FAILED:
      return "Authentication failed";
    case SERVER_ERROR:
      return "Server error (try again later)";
    case JSON_ERROR:
      return "JSON parse error";
    case NOT_FOUND:
      return "No progress found";
    case LOW_MEMORY:
      return "Not enough memory for sync — please retry";
    case CERTIFICATE_REJECTED:
      return tr(STR_CERTIFICATE_REJECTED);
    default:
      return "Unknown error";
  }
}
