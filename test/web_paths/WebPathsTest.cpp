#include <gtest/gtest.h>

#include <random>
#include <string>
#include <string_view>

#include "OldWebPaths.h"
#include "network/ProtectedPaths.h"

namespace {

// The new code as WebDAVHandler and CrossPointWebServer now compose it.
String requestPath(const String& uri) { return FsHelpers::webPath(old_webpaths::urlDecode(uri).c_str()).c_str(); }

String destinationPath(String dest) {
  if (dest.isEmpty()) return "";
  size_t start = 0;
  dest = FsHelpers::urlPathStart(dest.c_str(), start) ? dest.substring(start) : String("/");
  return FsHelpers::webPath(old_webpaths::urlDecode(dest).c_str()).c_str();
}

String webPath(const String& path) { return FsHelpers::webPath(path.c_str()).c_str(); }

String encoded(const String& path) {
  return FsHelpers::encodeUrlPath(std::string_view{path.c_str(), path.length()}).c_str();
}

std::string str(const String& s) { return {s.c_str(), s.length()}; }

// Pieces of paths and URLs that hit the rules: separators, dot names, system and store names,
// SdFat's trimming and aliases, escapes, schemes, non-ASCII bytes and a raw NUL (urlDecode turns
// "%00" in a request argument into one).
const std::string_view PIECES[] = {std::string_view("\0", 1),
                                   "/",
                                   "//",
                                   ".",
                                   "..",
                                   "...",
                                   " ",
                                   "a",
                                   "Books",
                                   ".crosspoint",
                                   ".CrossPoint",
                                   "wifi.json",
                                   "opds.json",
                                   "koreader.json",
                                   "WIFI.JSON",
                                   "wifi.json.tmp",
                                   "settings.json",
                                   "CROSSP~1",
                                   "WIFI~1.JSO",
                                   "IMG~1.JPG",
                                   "a~b",
                                   "~1",
                                   "x.~2",
                                   "System Volume Information",
                                   "XTCache",
                                   "xtcache",
                                   "%2F",
                                   "%2e%2E",
                                   "%20",
                                   "%",
                                   "%4",
                                   "%zz",
                                   "+",
                                   "?",
                                   "#",
                                   "&",
                                   "http://",
                                   "https://h:8",
                                   "://",
                                   "\xC3\xA7",
                                   "\xFF",
                                   ". ",
                                   " .",
                                   "wifi.json.",
                                   "wifi.json ",
                                   "%00",
                                   "=",
                                   "\\",
                                   "\x7F"};

std::string randomPath(std::mt19937& rng) {
  std::string out;
  for (int n = std::uniform_int_distribution<int>(0, 9)(rng); n > 0; --n) {
    out += PIECES[std::uniform_int_distribution<size_t>(0, std::size(PIECES) - 1)(rng)];
  }
  return out;
}

TEST(WebPaths, MatchTheOldChecks) {
  std::mt19937 rng(20261025);
  for (int i = 0; i < 300000; ++i) {
    const std::string raw = randomPath(rng);
    const String path(raw);
    const bool nul = raw.find('\0') != std::string::npos;
    SCOPED_TRACE(raw);
    ASSERT_EQ(protectedpaths::isSensitivePath(raw.c_str()), old_protectedpaths::isSensitivePath(raw.c_str()));
    ASSERT_EQ(protectedpaths::isPluginPath(raw), old_protectedpaths::isPluginPath(raw));
    // Past a NUL the old check saw one component to the end; the new one splits it and may refuse more.
    if (nul) {
      ASSERT_TRUE(protectedpaths::hasHiddenComponent(raw.data(), raw.size()) || !old_webpaths::isProtectedPath(path));
    } else {
      ASSERT_EQ(protectedpaths::hasHiddenComponent(raw.data(), raw.size()), old_webpaths::isProtectedPath(path));
    }
    ASSERT_EQ(protectedpaths::isHiddenName(raw.data(), raw.size()), old_webpaths::isProtectedItemName(path));
    bool system = false;
    for (const auto* item : old_webpaths::HIDDEN_ITEMS) system = system || path.equals(item);
    ASSERT_EQ(protectedpaths::isSystemName(raw.data(), raw.size()), system);
    ASSERT_EQ(str(webPath(path)), str(old_webpaths::normalizeWebPath(path)));
    ASSERT_EQ(str(requestPath(path)), str(old_webpaths::getRequestPath(path)));
    ASSERT_EQ(str(destinationPath(path)), str(old_webpaths::getDestinationPath(path)));
    String old;
    old_webpaths::urlEncodePath(path, old);
    // WebDAVHandler, like this helper, keeps the encoding up to a NUL (its paths have none); the old
    // code copied the NUL and what follows.
    ASSERT_EQ(str(encoded(path)), nul ? std::string(old.c_str()) : str(old));
  }
}

TEST(WebPaths, SystemNamesAreExact) {
  EXPECT_TRUE(protectedpaths::isSystemName("XTCache", 7));
  EXPECT_FALSE(protectedpaths::isSystemName("XTCache2", 8));
  EXPECT_FALSE(protectedpaths::isSystemName(".fonts", 6));
  EXPECT_TRUE(protectedpaths::isHiddenName(".fonts", 6));
  EXPECT_TRUE(protectedpaths::hasHiddenComponent("/Books/System Volume Information/x", 34));
  EXPECT_TRUE(protectedpaths::isSystemName("XTCache\0x", 9));
  EXPECT_TRUE(protectedpaths::isHiddenName("System Volume Information\0", 26));
}

TEST(WebPaths, DestinationsAndEncoding) {
  EXPECT_EQ(str(destinationPath("http://192.168.4.1/Books/a%20b.epub")), "/Books/a b.epub");
  EXPECT_EQ(str(destinationPath("http://host")), "/");
  EXPECT_EQ(str(destinationPath("/Books/../x/")), "/x");
  EXPECT_EQ(str(destinationPath("")), "");
  EXPECT_EQ(str(encoded("/Kitaplar/Çiçek #1?.epub")), "/Kitaplar/%C3%87i%C3%A7ek%20%231%3F.epub");
}

}  // namespace
