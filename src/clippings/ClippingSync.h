#pragma once

#include <string>

namespace freeink {
class SecureHttpClient;
}

namespace clippingSync {
// Disabled in KOSync settings by default. Returns false on an enabled sync failure;
// *certificateRejected then tells whether the server's certificate was refused.
// A non-null connection is used and left open, as in KOReaderSyncClient.
bool run(const std::string& bookPath, const std::string& documentHash, bool* certificateRejected = nullptr,
         freeink::SecureHttpClient* connection = nullptr);
}  // namespace clippingSync
