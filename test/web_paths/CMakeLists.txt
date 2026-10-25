add_executable(WebPathsTest
  WebPathsTest.cpp
  ${REPO_ROOT}/lib/FsHelpers/FsHelpers.cpp
  ${REPO_ROOT}/lib/Utf8/Utf8.cpp
)

target_include_directories(WebPathsTest PRIVATE
  ${REPO_ROOT}/test/fs_helpers/stubs
  ${REPO_ROOT}/src
  ${REPO_ROOT}/lib/FsHelpers
  ${REPO_ROOT}/lib/Utf8
)

target_link_libraries(WebPathsTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(WebPathsTest)
