add_executable(BidiUtilsTest
  BidiUtilsTest.cpp
  BidiCorpusTest.cpp
  ${REPO_ROOT}/lib/MiniBidi/BidiUtils.cpp
  ${REPO_ROOT}/lib/Utf8/Utf8.cpp
)
target_include_directories(BidiUtilsTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/lib/MiniBidi
  ${REPO_ROOT}/lib/Utf8
)
target_link_libraries(BidiUtilsTest PRIVATE crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(BidiUtilsTest)
