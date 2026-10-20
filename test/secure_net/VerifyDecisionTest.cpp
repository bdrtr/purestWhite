#include <SecureClient.h>
#include <gtest/gtest.h>

#include <string>

namespace {

std::string lastHost;
bool policyAnswer = false;

bool policy(const char* host) {
  lastHost = host;
  return policyAnswer;
}

using freeink::SecureClient;

TEST(SecureClientVerify, PinnedRootIsAlwaysChecked) {
  policyAnswer = false;
  EXPECT_TRUE(SecureClient::shouldVerify(true, true, policy, "home.lan"));
  EXPECT_TRUE(SecureClient::shouldVerify(true, true, nullptr, "home.lan"));
}

TEST(SecureClientVerify, PolicyOverridesSetInsecure) {
  // The SDK's OPDS sign-in still calls setInsecure(); the firmware's policy decides anyway.
  policyAnswer = true;
  lastHost.clear();
  EXPECT_TRUE(SecureClient::shouldVerify(true, false, policy, "login.example.com"));
  EXPECT_EQ(lastHost, "login.example.com");
  policyAnswer = false;
  EXPECT_FALSE(SecureClient::shouldVerify(false, false, policy, "home.lan"));
}

TEST(SecureClientVerify, WithoutPolicySetInsecureDecides) {
  EXPECT_TRUE(SecureClient::shouldVerify(false, false, nullptr, "example.com"));
  EXPECT_FALSE(SecureClient::shouldVerify(true, false, nullptr, "example.com"));
}

}  // namespace
