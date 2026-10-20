#pragma once

#include <cstdio>
#include <cstring>

// Release versions are "major.minor.patch", optionally after a "v" (a tag such as "v0.0.2").
namespace ota_version {

inline bool parse(const char* text, int& major, int& minor, int& patch) {
  if (!text) return false;
  if (*text == 'v' || *text == 'V') ++text;
  return sscanf(text, "%d.%d.%d", &major, &minor, &patch) == 3;
}

// Whether the release named latest should replace the running version current. An RC build
// ("-rc" in current) takes its final release of the same number. A version that does not
// parse is never offered.
inline bool isNewer(const char* latest, const char* current) {
  int lMajor, lMinor, lPatch, cMajor, cMinor, cPatch;
  if (!parse(latest, lMajor, lMinor, lPatch) || !parse(current, cMajor, cMinor, cPatch)) return false;
  if (lMajor != cMajor) return lMajor > cMajor;
  if (lMinor != cMinor) return lMinor > cMinor;
  if (lPatch != cPatch) return lPatch > cPatch;
  return strstr(current, "-rc") != nullptr;
}

}  // namespace ota_version
