// The web path checks as they were at 9843def2, verbatim but for their inputs becoming parameters:
// src/network/ProtectedPaths.h, CrossPointWebServer.cpp's normalizeWebPath and isProtectedItemName,
// and WebDAVHandler.cpp's request and destination paths, path encoding and protected-path check.
// urlDecode is Arduino-ESP32's WebServer::urlDecode. WebPathsTest compares the new code with them.
#pragma once

#include <FsHelpers.h>
#include <WString.h>

#include <cstdio>
#include <cstdlib>

#include <string.h>
#include <strings.h>

#include <string>

// Credential stores no web request (file manager, plugin endpoints) or plugin
// manifest may read or write: their passwords are obfuscated with a device
// key, so the files must stay on the device. Mirrors the getFilePath() of
// WifiCredentialStore, OpdsServerStore and KOReaderCredentialStore. Prefix
// match, so the stores' temp/backup siblings are covered too.
namespace old_protectedpaths {

inline constexpr const char* SENSITIVE_FILES[] = {"/.crosspoint/wifi.json", "/.crosspoint/opds.json",
                                                  "/.crosspoint/koreader.json"};

// SdFat also opens a name by its generated 8.3 alias (".crosspoint" is
// CROSSP~1), which no spelling check can map back to the long name.
inline bool isShortAlias(const char* first, const char* last) {
  const char* tilde = static_cast<const char*>(memchr(first, '~', static_cast<size_t>(last - first)));
  if (!tilde || tilde + 1 >= last || tilde[1] < '0' || tilde[1] > '9') return false;
  const char* dot = static_cast<const char*>(memchr(first, '.', static_cast<size_t>(last - first)));
  const size_t base = static_cast<size_t>((dot ? dot : last) - first);
  const size_t ext = dot ? static_cast<size_t>(last - dot - 1) : 0;
  return base <= 8 && ext <= 3;
}

inline bool isSensitivePath(const char* path) {
  // Canonicalise first ("//", "/./" and ".." would otherwise dodge the prefix
  // match while still opening the same file).
  std::string canon;
  canon.reserve(strlen(path) + 1);
  int depth = 0;
  for (const char* seg = path; *seg;) {
    while (*seg == '/') seg++;
    const char* end = strchr(seg, '/');
    const size_t len = end ? static_cast<size_t>(end - seg) : strlen(seg);
    if (len == 2 && seg[0] == '.' && seg[1] == '.') {
      const size_t slash = canon.rfind('/');
      canon.erase(slash == std::string::npos ? 0 : slash);
      if (depth > 0) depth--;
    } else {
      // SdFat skips leading spaces and drops trailing dots and spaces when it
      // opens a name, so compare the name it would actually open.
      const char* first = seg;
      const char* last = seg + len;
      while (first < last && *first == ' ') first++;
      while (last > first && (last[-1] == '.' || last[-1] == ' ')) last--;
      // The stores sit two levels deep; an alias further down cannot reach one.
      if (depth < 2 && isShortAlias(first, last)) return true;
      if (first < last) {
        canon += '/';
        canon.append(first, static_cast<size_t>(last - first));
        depth++;
      }
    }
    seg += len;
  }
  for (const char* file : SENSITIVE_FILES) {
    // FAT names are case-insensitive.
    if (strncasecmp(canon.c_str(), file, strlen(file)) == 0) return true;
  }
  return false;
}

// A plugin-supplied path (web request or device.json): absolute, no parent
// refs, not a credential store.
inline bool isPluginPath(const std::string& p) {
  return p.size() > 1 && p[0] == '/' && p.find("..") == std::string::npos && !isSensitivePath(p.c_str());
}

}  // namespace old_protectedpaths

namespace old_webpaths {

constexpr const char* HIDDEN_ITEMS[] = {"System Volume Information", "XTCache"};

inline String urlDecode(const String& text) {
  String decoded = "";
  char temp[] = "0x00";
  unsigned int len = text.length();
  unsigned int i = 0;
  while (i < len) {
    char decodedChar;
    char encodedChar = text.charAt(i++);
    if ((encodedChar == '%') && (i + 1 < len)) {
      temp[2] = text.charAt(i++);
      temp[3] = text.charAt(i++);
      decodedChar = strtol(temp, NULL, 16);
    } else if (encodedChar == '+') {
      decodedChar = ' ';
    } else {
      decodedChar = encodedChar;  // normal ascii char
    }
    decoded += decodedChar;
  }
  return decoded;
}

inline String normalizeWebPath(const String& inputPath) {
  if (inputPath.isEmpty() || inputPath == "/") {
    return "/";
  }
  std::string normalized = FsHelpers::normalisePath(inputPath.c_str());
  String result = normalized.c_str();
  if (result.isEmpty()) {
    return "/";
  }
  if (!result.startsWith("/")) {
    result = "/" + result;
  }
  if (result.length() > 1 && result.endsWith("/")) {
    result = result.substring(0, result.length() - 1);
  }
  return result;
}

inline bool isProtectedItemName(const String& name) {
  if (name.startsWith(".")) {
    return true;
  }
  for (const auto* item : HIDDEN_ITEMS) {
    if (name.equals(item)) {
      return true;
    }
  }
  return false;
}

inline String getRequestPath(const String& uri) {
  String decoded = urlDecode(uri);

  // Normalize using FsHelpers
  std::string normalized = FsHelpers::normalisePath(decoded.c_str());
  String result = normalized.c_str();

  if (result.isEmpty()) return "/";
  if (!result.startsWith("/")) result = "/" + result;

  // Remove trailing slash unless root
  if (result.length() > 1 && result.endsWith("/")) {
    result = result.substring(0, result.length() - 1);
  }

  return result;
}

inline String getDestinationPath(String dest) {
  if (dest.isEmpty()) return "";

  // Extract path from full URL: http://host/path -> /path
  // Find the third slash (after http://)
  int schemeEnd = dest.indexOf("://");
  if (schemeEnd >= 0) {
    int pathStart = dest.indexOf('/', schemeEnd + 3);
    if (pathStart >= 0) {
      dest = dest.substring(pathStart);
    } else {
      dest = "/";
    }
  }

  String decoded = urlDecode(dest);
  std::string normalized = FsHelpers::normalisePath(decoded.c_str());
  String result = normalized.c_str();

  if (result.isEmpty()) return "/";
  if (!result.startsWith("/")) result = "/" + result;

  // Remove trailing slash unless root
  if (result.length() > 1 && result.endsWith("/")) {
    result = result.substring(0, result.length() - 1);
  }

  return result;
}

inline void urlEncodePath(const String& path, String& out) {
  out = "";
  for (unsigned int i = 0; i < path.length(); i++) {
    char c = path.charAt(i);
    if (c == '/') {
      out += '/';
    } else if (c == ' ') {
      out += "%20";
    } else if (c == '%') {
      out += "%25";
    } else if (c == '#') {
      out += "%23";
    } else if (c == '?') {
      out += "%3F";
    } else if (c == '&') {
      out += "%26";
    } else if ((uint8_t)c > 127) {
      // Encode non-ASCII bytes
      char hex[4];
      snprintf(hex, sizeof(hex), "%%%02X", (uint8_t)c);
      out += hex;
    } else {
      out += c;
    }
  }
}

inline bool isProtectedPath(const String& path) {
  // Check every segment of the path, not just the last one.
  // This prevents access to e.g. /.hidden/somefile or /System Volume Information/foo
  int start = 0;
  while (start < (int)path.length()) {
    if (path.charAt(start) == '/') {
      start++;
      continue;
    }
    int end = path.indexOf('/', start);
    if (end == -1) end = path.length();

    String segment = path.substring(start, end);

    if (segment.startsWith(".")) return true;

    for (const auto* item : HIDDEN_ITEMS) {
      if (segment.equals(item)) return true;
    }

    start = end + 1;
  }

  return false;
}

}  // namespace old_webpaths
