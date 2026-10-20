#include "TlsPolicy.h"

#include <Arduino.h>
#include <Logging.h>
#include <SecureClient.h>
#include <TrustedTime.h>
#include <WiFi.h>

#include <cstdint>

#include "CrossPointSettings.h"

namespace {
// SNTP sends its first request up to 5 s after it starts (LWIP_SNTP_STARTUP_DELAY), then waits for
// DNS and the answer.
constexpr unsigned long CLOCK_WAIT_MS = 8000;

// Certificate dates need the wall clock. A clock that was never set (no RTC
// time, no SNTP answer yet) would refuse every server, so wait for the SNTP
// sync that the Wi-Fi connection started, or start one; a running sync is
// never restarted. After one wait in vain, SNTP is out of reach: later
// connections of this boot do not wait again.
void waitForClock() {
  static bool gaveUp = false;
  if (trustedtime::trustedNow() != 0 || gaveUp || WiFi.status() != WL_CONNECTED) return;
  trustedtime::startSync();
  const unsigned long deadline = millis() + CLOCK_WAIT_MS;
  while (trustedtime::trustedNow() == 0 && static_cast<long>(deadline - millis()) > 0) delay(100);
  if (trustedtime::trustedNow() != 0) return;
  gaveUp = true;
  LOG_ERR("TLS", "Clock not set: certificate dates will be refused");
}

bool verifies(const char* host) {
  if (!SETTINGS.verifyServerCertificates && !isBuiltInHost(host)) return false;
  waitForClock();
  return true;
}
}  // namespace

void installTlsPolicy() { freeink::SecureClient::setVerifyPolicy(verifies); }
