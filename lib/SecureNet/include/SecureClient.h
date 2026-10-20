#pragma once

// FreeInk SDK — TLS 1.3 secure client.
//
// WHY: the precompiled mbedTLS shipped in the ESP-IDF/pioarduino package has
// TLS 1.3 compiled out as empty stubs (PSA crypto prerequisites disabled), so
// WiFiClientSecure / esp_http_client cannot reach TLS-1.3-only servers
// (e.g. KOSync at kosync.ak-team.com:3042 — handshake fails with
// -0x7780 MBEDTLS_ERR_SSL_FATAL_ALERT_MESSAGE). A -D Kconfig flag can't change a
// precompiled .a, and a custom_sdkconfig rebuild fails on managed-component
// dependencies. The only fix that doesn't rebuild ESP-IDF is to bring our own
// TLS stack compiled from source: wolfSSL, which supports TLS 1.3 + PSA.
//
// SecureClient is an Arduino Client wrapping a wolfSSL session over a plain
// WiFiClient transport.
//
// The server is checked against ESP-IDF's certificate bundle (the roots
// esp-tls trusts): its chain must lead to one of them, and its certificate
// must name the host. wolfSSL checks the chain; ESP-IDF's bundle code (on
// mbedTLS) checks only its link to the bundle root.
//
// OPT-IN: enable with -DFREEINK_NET_WOLFSSL=1 and add wolfSSL to lib_deps. With
// the flag off, this compiles to an inert no-op (connectSecure() returns false)
// so the rest of the SDK builds without the wolfSSL dependency present.

#include <Arduino.h>
#include <Client.h>
#include <WiFiClient.h>

#include <functional>
#include <utility>

struct WOLFSSL_X509_STORE_CTX;

namespace freeink {

class SecureClient : public Client {
 public:
  SecureClient() = default;
  ~SecureClient() override;

  // Return true to abandon an in-progress connect.
  using AbortCallback = std::function<bool()>;

  // Certificate / verification configuration (applied before connect()).
  // setCACert() trusts that one PEM root instead of the bundle; setInsecure()
  // skips the check unless a verify policy is installed.
  void setCACert(const char* rootCA);
  void setInsecure(bool insecure = true);

  // Decides, per host, whether a client without a pinned root checks the
  // server. Once installed it overrides setInsecure(), so code that still
  // calls that (the SDK's OPDS sign-in) follows the policy too. Process-wide:
  // set it once, before the first connect(). It runs once per connect(),
  // before the TCP connect, and may block (see the abort callback below).
  using VerifyPolicy = bool (*)(const char* host);
  static void setVerifyPolicy(VerifyPolicy policy);

  // Whether a connection to host is checked: always with a pinned root;
  // otherwise the policy decides, and without one, setInsecure() does.
  static bool shouldVerify(bool insecure, bool pinnedRoot, VerifyPolicy policy, const char* host) {
    if (pinnedRoot) return true;
    return policy ? policy(host) : !insecure;
  }

  // True when the last connect() failed because the server's certificate was
  // refused.
  bool certificateRejected() const { return _certRejected; }

  // True when the most recent connect() of any SecureClient failed because the
  // server's certificate was refused: for callers that only see a failure from
  // code that owns its client (the SDK's OPDS sign-in). HTTPS runs on one task.
  static bool lastConnectRefused();

  // Polled during connect(): between the TCP connect and the handshake, on
  // every handshake round-trip, and between the two handshake attempts. A
  // caller that drives its own input loop can use this both to cancel and to
  // keep that loop running, the same way the read paths already do.
  //
  // It cannot interrupt the blocking TCP connect itself, so the longest
  // uninterruptible stretch is one setConnectionTimeout() window rather than
  // the four that a failing connect() can otherwise spend. The verify policy
  // runs before it and is not interruptible either (the firmware's policy
  // waits for a clock that was never set).
  void setAbortCallback(AbortCallback cb) { _shouldAbort = std::move(cb); }

  // True when the last connect() returned because the abort callback fired
  // rather than because the connection failed.
  bool aborted() const { return _aborted; }

  // Connect and perform a TLS 1.3 handshake to host:port (uses the SNI host).
  int connect(IPAddress ip, uint16_t port) override;
  int connect(const char* host, uint16_t port) override;

  size_t write(uint8_t b) override;
  size_t write(const uint8_t* buf, size_t size) override;
  int available() override;
  int read() override;
  int read(uint8_t* buf, size_t size) override;
  int peek() override;
  void flush() override;
  void stop() override;
  uint8_t connected() override;
  operator bool() override { return connected(); }

  // True if the library was built with wolfSSL TLS 1.3 support enabled.
  static bool tls13Available();

 private:
  int connectWithMethod(const char* host, uint16_t port, bool verify, bool resume, void* method, const char* label);

  // wolfSSL's verify callback, called for each certificate that failed a check.
  static int verifyPeer(int preverified, WOLFSSL_X509_STORE_CTX* store);

  // Polls _shouldAbort and latches _aborted so callers can tell a cancel from
  // a failure after connect() returns 0.
  bool abortRequested();

  WiFiClient _transport;
  AbortCallback _shouldAbort;
  bool _aborted = false;
  const char* _rootCA = nullptr;
  bool _insecure = false;
  bool _certRejected = false;
  bool _certFailed = false;  // refused or not checked: the TLS 1.2 retry cannot help
  // Resumption key of the current connection (see "TLS resumption" in SecureClient.cpp); its
  // session is kept when the connection closes if the handshake succeeded, no fatal error followed
  // and the server sent a ticket.
  char _sessionHost[64] = {};
  uint16_t _sessionPort = 0;
  bool _sessionVerify = false;
  bool _sessionKeyed = false;
  bool _offeredSession = false;  // this handshake offered a kept session
  bool _sessionUsable = false;
  bool _gotTicket = false;
  void* _ssl = nullptr;  // WOLFSSL* (opaque to keep wolfSSL headers out of here)
  void* _ctx = nullptr;  // WOLFSSL_CTX*
  bool _connected = false;
};

}  // namespace freeink
