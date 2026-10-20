#pragma once

#include <cctype>
#include <string_view>

// Which HTTPS servers have their certificate checked (SecureClient::setVerifyPolicy):
// every server while "Verify Server Certificates" is on, and the firmware's own
// services always. The setting is read on every connection.

// The firmware's own services: CrossPoint's sync server and plugin hub,
// FreeInk's LCP service, and GitHub with its download hosts, which serve fonts
// and releases.
inline constexpr std::string_view BUILT_IN_TLS_DOMAINS[] = {"crosspointreader.com", "freeink.org", "github.com",
                                                            "githubusercontent.com"};

// True when host is one of BUILT_IN_TLS_DOMAINS or a subdomain of one.
inline bool isBuiltInHost(const std::string_view host) {
  for (const std::string_view domain : BUILT_IN_TLS_DOMAINS) {
    if (host.size() < domain.size()) continue;
    const size_t start = host.size() - domain.size();
    if (start > 0 && host[start - 1] != '.') continue;
    bool same = true;
    for (size_t i = 0; i < domain.size() && same; ++i) {
      same = std::tolower(static_cast<unsigned char>(host[start + i])) == domain[i];
    }
    if (same) return true;
  }
  return false;
}

// Hands the policy to SecureClient. Call once at boot, before any connection.
void installTlsPolicy();
