#include "OtaVersion.h"

#include <gtest/gtest.h>

using ota_version::isNewer;

TEST(OtaVersion, ComparesEachPartInOrder) {
  EXPECT_TRUE(isNewer("0.0.2", "0.0.1"));
  EXPECT_TRUE(isNewer("0.1.0", "0.0.9"));
  EXPECT_TRUE(isNewer("1.0.0", "0.9.9"));
  EXPECT_TRUE(isNewer("0.0.10", "0.0.9"));
  EXPECT_FALSE(isNewer("0.0.1", "0.0.1"));
  EXPECT_FALSE(isNewer("0.0.1", "0.0.2"));
  EXPECT_FALSE(isNewer("0.9.9", "1.0.0"));
}

TEST(OtaVersion, AcceptsATagWithV) {
  EXPECT_TRUE(isNewer("v0.0.2", "0.0.1"));
  EXPECT_FALSE(isNewer("v0.0.1", "0.0.1"));
}

TEST(OtaVersion, ReleaseCandidateTakesItsRelease) {
  EXPECT_TRUE(isNewer("0.0.2", "0.0.2-rc+abc1234"));
  EXPECT_FALSE(isNewer("0.0.1", "0.0.2-rc+abc1234"));
}

TEST(OtaVersion, NeverOffersAVersionThatDoesNotParse) {
  EXPECT_FALSE(isNewer("nightly", "0.0.1"));
  EXPECT_FALSE(isNewer("1.2", "0.0.1"));
  EXPECT_FALSE(isNewer("", "0.0.1"));
  EXPECT_FALSE(isNewer(nullptr, "0.0.1"));
  EXPECT_FALSE(isNewer("0.0.2", "dev"));
}
