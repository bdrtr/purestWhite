#include "SecureClient.h"

// wolfSSL is only pulled in when explicitly enabled. This keeps the default SDK
// build free of the wolfSSL dependency while leaving a single, well-defined
// integration point for the TLS 1.3 transport.
#if defined(FREEINK_NET_WOLFSSL)
#include <esp_crt_bundle.h>
#include <esp_heap_caps.h>
#include <esp_idf_version.h>
#include <mbedtls/x509_crt.h>
#include <wolfssl/error-ssl.h>
#include <wolfssl/ssl.h>
#include <wolfssl/wolfcrypt/error-crypt.h>

#include <array>
#include <ctime>
#include <memory>
#include <new>

// ESP-IDF's bundle check (esp_crt_bundle.c). It is exported for mbedTLS's verify hook but not
// declared in esp_crt_bundle.h, so its signature and contract are taken from IDF 5.x.
#if ESP_IDF_VERSION_MAJOR != 5
#error "Check esp_crt_verify_callback against this ESP-IDF version"
#endif
extern "C" int esp_crt_verify_callback(void* buf, mbedtls_x509_crt* crt, int depth, uint32_t* flags);

// P-384 roots and intermediates (ISRG Root X2, GTS Root R4, Sectigo E46) sign with SHA-384;
// without it wolfSSL refuses those chains (HASH_TYPE_E).
#if !defined(WOLFSSL_SHA384)
#error "SecureClient needs WOLFSSL_SHA384 (see platformio.ini)"
#endif
#endif

namespace freeink {

bool SecureClient::tls13Available() {
#if defined(FREEINK_NET_WOLFSSL)
  return true;
#else
  return false;
#endif
}

SecureClient::~SecureClient() { stop(); }

namespace {
SecureClient::VerifyPolicy s_verifyPolicy = nullptr;
bool s_lastConnectRefused = false;
}  // namespace

void SecureClient::setCACert(const char* rootCA) { _rootCA = rootCA; }
void SecureClient::setInsecure(const bool insecure) { _insecure = insecure; }
void SecureClient::setVerifyPolicy(const VerifyPolicy policy) { s_verifyPolicy = policy; }
bool SecureClient::lastConnectRefused() { return s_lastConnectRefused; }

#if defined(FREEINK_NET_WOLFSSL)

namespace {
// Bridge wolfSSL's I/O to the underlying WiFiClient transport.
int wcSend(WOLFSSL* /*ssl*/, char* buf, int sz, void* ctx) {
  auto* t = static_cast<WiFiClient*>(ctx);
  const int n = t->write(reinterpret_cast<const uint8_t*>(buf), sz);
  if (n <= 0) {
    // A dead transport must surface as CONN_CLOSE: mapping it to WANT_WRITE
    // makes the handshake spin until the deadline instead of failing fast.
    if (!t->connected()) return WOLFSSL_CBIO_ERR_CONN_CLOSE;
    return WOLFSSL_CBIO_ERR_WANT_WRITE;
  }
  return n;
}
int wcRecv(WOLFSSL* /*ssl*/, char* buf, int sz, void* ctx) {
  auto* t = static_cast<WiFiClient*>(ctx);
  if (!t->connected() && t->available() == 0) return WOLFSSL_CBIO_ERR_CONN_CLOSE;
  if (t->available() == 0) return WOLFSSL_CBIO_ERR_WANT_READ;
  const int n = t->read(reinterpret_cast<uint8_t*>(buf), sz);
  if (n <= 0) return WOLFSSL_CBIO_ERR_WANT_READ;
  return n;
}

bool isWantIo(const int err) {
  // Only the wolfSSL_get_error() codes. The WOLFSSL_CBIO_ERR_* callback return
  // codes never come out of wolfSSL_get_error and collide with fatal wolfCrypt
  // errors (MP_MEM is -2 == WOLFSSL_CBIO_ERR_WANT_READ); matching them here made
  // an out-of-memory handshake spin until the deadline instead of failing fast.
  return err == WOLFSSL_ERROR_WANT_READ || err == WOLFSSL_ERROR_WANT_WRITE;
}

// mbedTLS allocates from internal RAM only (CONFIG_MBEDTLS_INTERNAL_MEM_ALLOC). Below this the
// bundle check would fail for want of memory, which is not a refusal.
constexpr size_t MIN_INTERNAL_FREE = 12 * 1024;
constexpr size_t MIN_INTERNAL_BLOCK = 2 * 1024;

// wolfSSL leaves the dates of a certificate it found no signer for unchecked, and mbedTLS here
// keeps no clock (MBEDTLS_HAVE_TIME_DATE is off).
bool withinValidity(const mbedtls_x509_crt& crt) {
  const time_t now = time(nullptr);
  tm utc;
  if (!gmtime_r(&now, &utc)) return false;
  const std::array<int, 6> today = {utc.tm_year + 1900, utc.tm_mon + 1, utc.tm_mday,
                                    utc.tm_hour,        utc.tm_min,     utc.tm_sec};
  const auto fields = [](const mbedtls_x509_time& t) {
    return std::array<int, 6>{t.year, t.mon, t.day, t.hour, t.min, t.sec};
  };
  return fields(crt.valid_from) <= today && today <= fields(crt.valid_to);
}

enum class BundleCheck : uint8_t { Accepted, Refused, NotChecked };

// No CA is loaded, so wolfSSL finds no signer for the topmost certificate the server sent. ESP-IDF
// then looks that certificate's issuer up among the bundle's roots and checks its signature, as
// for esp-tls. Once it is accepted, wolfSSL holds it as a CA and checks the rest of the chain and
// the host name itself.
BundleCheck checkAgainstBundle(const WOLFSSL_X509_STORE_CTX& store) {
  // Not the server's own certificate: a root signs intermediates only.
  if (store.error != ASN_NO_SIGNER_E || store.error_depth < 1 || store.error_depth >= store.totalCerts) {
    return BundleCheck::Refused;
  }
  if (heap_caps_get_free_size(MALLOC_CAP_INTERNAL) < MIN_INTERNAL_FREE ||
      heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL) < MIN_INTERNAL_BLOCK) {
    return BundleCheck::NotChecked;
  }
  std::unique_ptr<mbedtls_x509_crt> crt(new (std::nothrow) mbedtls_x509_crt);
  if (!crt) return BundleCheck::NotChecked;
  const WOLFSSL_BUFFER_INFO& der = store.certs[store.error_depth];
  mbedtls_x509_crt_init(crt.get());
  const int parsed = mbedtls_x509_crt_parse_der_nocopy(crt.get(), der.buffer, der.length);
  BundleCheck result = BundleCheck::Refused;
  if (parsed == MBEDTLS_ERR_X509_ALLOC_FAILED) {
    result = BundleCheck::NotChecked;
  } else if (parsed == 0 && withinValidity(*crt)) {
    uint32_t flags = MBEDTLS_X509_BADCERT_NOT_TRUSTED;
    if (esp_crt_verify_callback(nullptr, crt.get(), store.error_depth, &flags) == 0 && flags == 0) {
      result = BundleCheck::Accepted;
    }
  }
  mbedtls_x509_crt_free(crt.get());
  return result;
}

// Failures that say nothing about the certificate.
bool outOfMemory(const int error) { return error == MEMORY_E || error == MEMORY_ERROR; }
}  // namespace

int SecureClient::verifyPeer(const int preverified, WOLFSSL_X509_STORE_CTX* store) {
  if (preverified) return 1;
  auto* client = static_cast<SecureClient*>(store->userCtx);
  if (!client) return 0;
  // A pinned root replaces the bundle.
  const BundleCheck check = client->_rootCA ? BundleCheck::Refused : checkAgainstBundle(*store);
  if (check == BundleCheck::Accepted) return 1;
  client->_certFailed = true;
  if (check == BundleCheck::NotChecked || outOfMemory(store->error)) {
    if (Serial) Serial.printf("[SecureClient] certificate %d not checked: low memory\n", store->error_depth);
    return 0;
  }
  if (Serial) Serial.printf("[SecureClient] certificate %d refused: %d\n", store->error_depth, store->error);
  client->_certRejected = true;
  s_lastConnectRefused = true;
  return 0;
}

int SecureClient::connectWithMethod(const char* host, uint16_t port, const bool verify, void* method,
                                    const char* label) {
#if defined(FREEINK_WOLFSSL_DEBUG)
  // Routes wolfSSL's internal trace through wolfSSL_Arduino_Serial_Print (the
  // application provides that hook). Shows exactly where a handshake stalls.
  static bool debugEnabled = false;
  if (!debugEnabled) {
    wolfSSL_Debugging_ON();
    debugEnabled = true;
  }
#endif
  const uint32_t started = millis();
  stop();
  const uint32_t timeoutMs = getTimeout();
  if (abortRequested()) return 0;
  _transport.setConnectionTimeout(timeoutMs);
  if (!_transport.connect(host, port)) {
    if (Serial) Serial.printf("[SecureClient] TCP connect failed (%s): %s:%u\n", label, host, port);
    return 0;
  }
  if (abortRequested()) {
    _transport.stop();
    return 0;
  }

  auto* ctx = wolfSSL_CTX_new(static_cast<WOLFSSL_METHOD*>(method));
  if (!ctx) {
    if (Serial)
      Serial.printf("[SecureClient] CTX alloc failed (%s), free heap %u\n", label, (unsigned)ESP.getFreeHeap());
    _transport.stop();
    return 0;
  }
  _ctx = ctx;

  if (!verify) {
    wolfSSL_CTX_set_verify(ctx, WOLFSSL_VERIFY_NONE, nullptr);
  } else {
    if (_rootCA) {
      wolfSSL_CTX_load_verify_buffer(ctx, reinterpret_cast<const unsigned char*>(_rootCA), strlen(_rootCA),
                                     WOLFSSL_FILETYPE_PEM);
    } else if (esp_crt_bundle_attach(nullptr) != ESP_OK) {
      // Points ESP-IDF's check at the bundle linked into the image; a no-op once it is.
      if (Serial) Serial.println("[SecureClient] certificate bundle unavailable");
      stop();
      return 0;
    }
    wolfSSL_CTX_set_verify(ctx, WOLFSSL_VERIFY_PEER, verifyPeer);
  }
  wolfSSL_SetIORecv(ctx, wcRecv);
  wolfSSL_SetIOSend(ctx, wcSend);

  auto* ssl = wolfSSL_new(ctx);
  if (!ssl) {
    if (Serial)
      Serial.printf("[SecureClient] SSL alloc failed (%s), free heap %u\n", label, (unsigned)ESP.getFreeHeap());
    stop();
    return 0;
  }
  _ssl = ssl;
  wolfSSL_SetIOReadCtx(ssl, &_transport);
  wolfSSL_SetIOWriteCtx(ssl, &_transport);
  wolfSSL_UseSNI(ssl, WOLFSSL_SNI_HOST_NAME, host, strlen(host));
  if (verify) {
    wolfSSL_SetCertCbCtx(ssl, this);
    // wolfSSL compares the certificate with the host name only when asked to.
    if (wolfSSL_check_domain_name(ssl, host) != WOLFSSL_SUCCESS) {
      stop();
      return 0;
    }
  }
#if defined(WOLFSSL_TLS13) && defined(HAVE_CURVE25519)
  // MEMFIX-PORT: pin the TLS 1.3 key_share to X25519. wolfSSL's default is a
  // P-256 share, generated with fast-math bignums that WOLFSSL_SMALL_STACK
  // heap-allocates at ~4KB apiece (FP_MAX_BITS-sized) -- at the free-heap
  // levels a reading session leaves (~45-50KB) that keygen fails with MP_MEM
  // and the ClientHello is never sent. Curve25519 uses fixed 32-byte field
  // arithmetic with no bignum temporaries. A server without X25519 answers
  // with a HelloRetryRequest and wolfSSL falls back to its group list.
  wolfSSL_UseKeyShare(ssl, WOLFSSL_ECC_X25519);
#endif
#ifdef HAVE_MAX_FRAGMENT
  // Ask the peer to cap TLS records at 2KB (RFC 6066 max_fragment_length).
  // wolfSSL sizes its receive buffer to each incoming record, so a default
  // 16KB record demands a ~17KB contiguous allocation per record -- measured
  // failing (MEMORY_E mid-download) at the ~20KB free heap a busy activity
  // leaves. With 2KB records the receive buffer stays trivial. Servers that
  // ignore the extension keep 16KB records and behave as before.
  wolfSSL_UseMaxFragment(ssl, WOLFSSL_MFL_2_11);
#endif

  // The recv callback is non-blocking (returns WANT_READ when no bytes are
  // buffered), so wolfSSL_connect must be retried across handshake round-trips
  // rather than called once.
  const uint32_t deadline = millis() + timeoutMs;
  int ret;
  while ((ret = wolfSSL_connect(ssl)) != WOLFSSL_SUCCESS) {
    const int err = wolfSSL_get_error(ssl, ret);
    if (!isWantIo(err)) {
      if (Serial) Serial.printf("[SecureClient] wolfSSL_connect failed (%s): %d\n", label, err);
      stop();
      return 0;
    }
    if (abortRequested()) {
      if (Serial) Serial.printf("[SecureClient] handshake aborted by caller (%s)\n", label);
      stop();
      return 0;
    }
    if (static_cast<int32_t>(millis() - deadline) >= 0) {
      if (Serial) {
        Serial.printf("[SecureClient] handshake timeout (%s): last err %d, transport %s, free heap %u\n", label, err,
                      _transport.connected() ? "up" : "down", (unsigned)ESP.getFreeHeap());
      }
      stop();
      return 0;
    }
    delay(5);
  }
  _connected = true;
  if (Serial) {
    Serial.printf("[SecureClient] handshake ok (%s): %s / %s in %lu ms\n", label, wolfSSL_get_version(ssl),
                  wolfSSL_get_cipher(ssl), (unsigned long)(millis() - started));
  }
  return 1;
}

bool SecureClient::abortRequested() {
  if (!_shouldAbort || !_shouldAbort()) return false;
  _aborted = true;
  return true;
}

int SecureClient::connect(const char* host, uint16_t port) {
  _aborted = false;
  _certRejected = false;
  _certFailed = false;
  s_lastConnectRefused = false;
  // Decided once, before any socket: the policy may wait for the clock.
  const bool verify = shouldVerify(_insecure, _rootCA != nullptr, s_verifyPolicy, host);
  if (abortRequested()) return 0;
  // Negotiate the highest mutually supported version rather than pinning TLS 1.3:
  // self-hosted / Let's Encrypt nginx often tops out at TLS 1.2, and a 1.3-only
  // client fails those handshakes outright. v23 still selects 1.3 when the peer
  // offers it (WOLFSSL_TLS13 is enabled) and falls back to 1.2 otherwise.
  if (connectWithMethod(host, port, verify, wolfSSLv23_client_method(), "auto")) return 1;
  // The retry is for servers that reject a TLS 1.3 ClientHello; one that got
  // as far as its certificate fails the same way under TLS 1.2.
  if (_aborted || _certFailed) return 0;

  // Some TLS 1.2-only servers are intolerant of a TLS 1.3-capable ClientHello
  // and abort with a fatal handshake_failure alert. Retry with an explicit
  // TLS 1.2 ClientHello before giving up.
  if (Serial) Serial.println("[SecureClient] retrying with TLS 1.2-only handshake");
  return connectWithMethod(host, port, verify, wolfTLSv1_2_client_method(), "tls1.2");
}

int SecureClient::connect(IPAddress ip, uint16_t port) {
  char host[16];
  snprintf(host, sizeof(host), "%u.%u.%u.%u", ip[0], ip[1], ip[2], ip[3]);
  return connect(host, port);
}

size_t SecureClient::write(const uint8_t* buf, size_t size) {
  if (!_connected) return 0;
  const int n = wolfSSL_write(static_cast<WOLFSSL*>(_ssl), buf, size);
  return n > 0 ? static_cast<size_t>(n) : 0;
}

int SecureClient::read(uint8_t* buf, size_t size) {
  if (!_connected) return -1;
  auto* ssl = static_cast<WOLFSSL*>(_ssl);
  const int n = wolfSSL_read(ssl, buf, size);
  if (n > 0) return n;

  const int err = wolfSSL_get_error(ssl, n);
  if (isWantIo(err)) return 0;
  if (err == WOLFSSL_ERROR_ZERO_RETURN) {
    _connected = false;
    return 0;
  }
  // A mid-stream failure is invisible to callers (they just see the connection
  // die); the error code distinguishes an OOM (MEMORY_E -125) from a peer
  // drop or MAC failure.
  if (Serial) {
    Serial.printf("[SecureClient] read failed: %d, free heap %u, max block %u\n", err, (unsigned)ESP.getFreeHeap(),
                  (unsigned)ESP.getMaxAllocHeap());
  }
  _connected = false;
  return -1;
}

int SecureClient::available() {
  if (!_connected) return 0;
  return wolfSSL_pending(static_cast<WOLFSSL*>(_ssl)) + _transport.available();
}

void SecureClient::stop() {
  if (_ssl) {
    wolfSSL_free(static_cast<WOLFSSL*>(_ssl));
    _ssl = nullptr;
  }
  if (_ctx) {
    wolfSSL_CTX_free(static_cast<WOLFSSL_CTX*>(_ctx));
    _ctx = nullptr;
  }
  _transport.stop();
  _connected = false;
}

uint8_t SecureClient::connected() { return _connected && _transport.connected(); }

#else  // !FREEINK_NET_WOLFSSL — inert stub so the SDK builds without wolfSSL.

int SecureClient::connect(const char* host, uint16_t port) {
  (void)host;
  (void)port;
  if (Serial) Serial.println("[SecureClient] TLS 1.3 unavailable: build with -DFREEINK_NET_WOLFSSL=1");
  return 0;
}
int SecureClient::connect(IPAddress ip, uint16_t port) {
  (void)ip;
  (void)port;
  return 0;
}
size_t SecureClient::write(const uint8_t* buf, size_t size) {
  (void)buf;
  (void)size;
  return 0;
}
int SecureClient::read(uint8_t* buf, size_t size) {
  (void)buf;
  (void)size;
  return -1;
}
int SecureClient::available() { return 0; }
void SecureClient::stop() {
  _transport.stop();
  _connected = false;
}
uint8_t SecureClient::connected() { return 0; }

#endif

// --- transport-agnostic single-byte helpers (shared) ---
size_t SecureClient::write(uint8_t b) { return write(&b, 1); }
int SecureClient::read() {
  uint8_t b;
  return read(&b, 1) == 1 ? b : -1;
}
int SecureClient::peek() { return -1; }
void SecureClient::flush() {}

}  // namespace freeink
