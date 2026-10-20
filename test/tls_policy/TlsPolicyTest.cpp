#include <gtest/gtest.h>

#include "TlsPolicy.h"

namespace {

TEST(TlsPolicy, BuiltInDomainsAndTheirSubdomains) {
  EXPECT_TRUE(isBuiltInHost("crosspointreader.com"));
  EXPECT_TRUE(isBuiltInHost("sync.crosspointreader.com"));
  EXPECT_TRUE(isBuiltInHost("pluginhub.crosspointreader.com"));
  EXPECT_TRUE(isBuiltInHost("lcp.freeink.org"));
  EXPECT_TRUE(isBuiltInHost("github.com"));
  EXPECT_TRUE(isBuiltInHost("api.github.com"));
  EXPECT_TRUE(isBuiltInHost("release-assets.githubusercontent.com"));
}

TEST(TlsPolicy, HostNamesIgnoreCase) {
  EXPECT_TRUE(isBuiltInHost("Sync.CrossPointReader.COM"));
  EXPECT_TRUE(isBuiltInHost("GITHUB.COM"));
}

TEST(TlsPolicy, OnlyWholeLabelsMatch) {
  EXPECT_FALSE(isBuiltInHost("evilcrosspointreader.com"));
  EXPECT_FALSE(isBuiltInHost("notgithub.com"));
  EXPECT_FALSE(isBuiltInHost("crosspointreader.com.example.net"));
  EXPECT_FALSE(isBuiltInHost("freeink.org.example.net"));
  EXPECT_FALSE(isBuiltInHost("crosspointreader.co"));
  // A trailing root dot is another spelling the policy does not claim.
  EXPECT_FALSE(isBuiltInHost("github.com."));
}

TEST(TlsPolicy, OtherHostsFollowTheSetting) {
  EXPECT_FALSE(isBuiltInHost(""));
  EXPECT_FALSE(isBuiltInHost("sync.koreader.rocks"));
  EXPECT_FALSE(isBuiltInHost("192.168.1.10"));
  EXPECT_FALSE(isBuiltInHost("calibre.local"));
}

}  // namespace
