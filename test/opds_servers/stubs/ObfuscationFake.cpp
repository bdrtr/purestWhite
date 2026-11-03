// Host stand-in for lib/Serialization/ObfuscationUtils.cpp (which is keyed by the device's eFuse
// MAC): "~" and the text in hex, so the stores' answers can be read. It keeps the real one's
// interface rules: empty text encodes as empty; empty, missing or malformed input does not decode
// (ok false); a decoded length past maxDecodedLength sets tooLong.
#include <ObfuscationUtils.h>

#include <cstring>
#include <limits>

namespace obfuscation {

String obfuscateToBase64(const std::string& plaintext) {
  if (plaintext.empty()) return String("");
  std::string out = "~";
  char b[3];
  for (const unsigned char c : plaintext) {
    std::snprintf(b, sizeof(b), "%02x", c);
    out += b;
  }
  return String(out.c_str());
}

std::string deobfuscateFromBase64(const char* encoded, bool* ok) {
  return deobfuscateFromBase64(encoded, std::numeric_limits<size_t>::max(), ok, nullptr);
}

std::string deobfuscateFromBase64(const char* encoded, const size_t maxDecodedLength, bool* ok, bool* tooLong) {
  if (tooLong) *tooLong = false;
  if (ok) *ok = false;
  if (encoded == nullptr || encoded[0] != '~') return "";
  const size_t len = std::strlen(encoded + 1);
  if (len % 2 != 0) return "";
  std::string out;
  for (size_t i = 0; i < len; i += 2) {
    const char pair[3] = {encoded[1 + i], encoded[2 + i], '\0'};
    char* end = nullptr;
    const long value = std::strtol(pair, &end, 16);
    if (end != pair + 2) return "";
    out.push_back(static_cast<char>(value));
  }
  if (out.size() > maxDecodedLength) {
    if (tooLong) *tooLong = true;
    return "";
  }
  if (ok) *ok = true;
  return out;
}

}  // namespace obfuscation
