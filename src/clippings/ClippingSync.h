#pragma once

#include <string>

namespace clippingSync {
// Disabled in KOSync settings by default. Returns false on an enabled sync failure;
// *certificateRejected then tells whether the server's certificate was refused.
bool run(const std::string& bookPath, const std::string& documentHash, bool* certificateRejected = nullptr);
}  // namespace clippingSync
