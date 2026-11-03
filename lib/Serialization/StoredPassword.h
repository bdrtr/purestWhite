#pragma once

#include <cstddef>
#include <string>

// A password as the stores keep it on the card: obfuscated with the device's key
// (ObfuscationUtils) in "password_obf", or, in files from before that, in plain text in
// "password". Both texts are the stored fields ("" when missing).
namespace obfuscation {

// The password: password_obf decoded, else (when it does not decode) the legacy plain one, and
// needsResave set when that one is not empty, so the store saves it obfuscated. A decoded or
// legacy password longer than maxLength reads as "" with valid false.
std::string readStoredPassword(const char* passwordObf, const char* legacyPassword, size_t maxLength, bool& needsResave,
                               bool& valid);

// The same with no length limit.
std::string readStoredPassword(const char* passwordObf, const char* legacyPassword, bool& needsResave);

}  // namespace obfuscation
