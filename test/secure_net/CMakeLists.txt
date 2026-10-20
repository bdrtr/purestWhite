# lib/SecureNet's own host test: fetchResumable over scripted plain-http replies
# (stubs/ fakes WiFiClient). It checks with assert(), so NDEBUG stays off.
set(SECURE_NET_TEST_DIR ${REPO_ROOT}/lib/SecureNet/test/host)

add_executable(SecureNetResumableFetchTest
  ${SECURE_NET_TEST_DIR}/test_resumable_fetch.cpp
)

target_include_directories(SecureNetResumableFetchTest PRIVATE
  ${SECURE_NET_TEST_DIR}/stubs
  ${REPO_ROOT}/lib/SecureNet/include
)

target_compile_options(SecureNetResumableFetchTest PRIVATE -UNDEBUG -Wall -Wextra -Werror)

add_test(NAME SecureNetResumableFetch COMMAND SecureNetResumableFetchTest)

add_executable(SecureClientVerifyTest
  VerifyDecisionTest.cpp
)

target_include_directories(SecureClientVerifyTest PRIVATE
  ${SECURE_NET_TEST_DIR}/stubs
  ${REPO_ROOT}/lib/SecureNet/include
)

target_link_libraries(SecureClientVerifyTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(SecureClientVerifyTest)
