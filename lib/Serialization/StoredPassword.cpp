#include "StoredPassword.h"

#include <ObfuscationUtils.h>

#include <cstring>
#include <limits>

namespace obfuscation {

std::string readStoredPassword(const char* passwordObf, const char* legacyPassword, const size_t maxLength,
                               bool& needsResave, bool& valid) {
  valid = true;
  bool ok = false;
  bool tooLong = false;
  std::string pass = deobfuscateFromBase64(passwordObf, maxLength, &ok, &tooLong);
  if (tooLong) {
    valid = false;
    return "";
  }
  if (!ok) {
    // Deobfuscation failed — fall back to legacy plaintext password.
    const size_t legacyLength = strlen(legacyPassword);
    if (legacyLength > maxLength) {
      valid = false;
      return "";
    }
    pass.assign(legacyPassword, legacyLength);
    if (!pass.empty()) needsResave = true;
  }
  // A successfully decoded empty string is a legitimate value; preserve as-is.
  return pass;
}

std::string readStoredPassword(const char* passwordObf, const char* legacyPassword, bool& needsResave) {
  bool valid = false;
  return readStoredPassword(passwordObf, legacyPassword, std::numeric_limits<size_t>::max(), needsResave, valid);
}

}  // namespace obfuscation
