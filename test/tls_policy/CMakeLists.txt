add_executable(TlsPolicyTest
  TlsPolicyTest.cpp
)

target_include_directories(TlsPolicyTest PRIVATE
  ${REPO_ROOT}/src/network
)

target_link_libraries(TlsPolicyTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(TlsPolicyTest)
