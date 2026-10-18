// Which of NimBLE's bonds go before a new remote pairs, so its bond never evicts a listed remote's.

#include <gtest/gtest.h>

#include "RadioPolicy.h"

namespace bleturner {
namespace {

constexpr const char* kFirst = "aa:00:00:00:00:01";
constexpr const char* kSecond = "aa:00:00:00:00:02";
constexpr const char* kLeftOver = "aa:00:00:00:00:07";  // bonded, its HID setup failed
constexpr const char* kNew = "aa:00:00:00:00:09";

TEST(StaleBondTest, ABondTheSdkDoesNotListGoesBeforeANewRemote) {
  const char* listed[] = {kFirst, kSecond};
  EXPECT_TRUE(staleBond(kLeftOver, kNew, listed, 2));
  EXPECT_FALSE(staleBond(kFirst, kNew, listed, 2)) << "a listed remote keeps its keys";
  EXPECT_FALSE(staleBond(kSecond, kNew, listed, 2));
}

TEST(StaleBondTest, TheTargetKeepsItsOwnBond) {
  const char* listed[] = {kFirst};
  EXPECT_FALSE(staleBond(kLeftOver, kLeftOver, listed, 1)) << "pairing again may reuse its keys";
}

TEST(StaleBondTest, AListedTargetDropsNothing) {
  // Connecting to a listed remote adds no bond, so nothing can be evicted for it.
  const char* listed[] = {kFirst, kSecond};
  EXPECT_FALSE(staleBond(kLeftOver, kFirst, listed, 2));
}

TEST(StaleBondTest, WithNothingListedEveryOtherBondGoes) {
  // A store left full by an older build: the SDK lists none of it.
  EXPECT_TRUE(staleBond(kFirst, kNew, nullptr, 0));
  EXPECT_FALSE(staleBond(kNew, kNew, nullptr, 0));
}

TEST(StaleBondTest, OnlyTheListedCountIsRead) {
  const char* listed[] = {kFirst, kSecond};
  EXPECT_TRUE(staleBond(kSecond, kNew, listed, 1)) << "the second entry is past the count";
}

}  // namespace
}  // namespace bleturner
