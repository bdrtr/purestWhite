add_executable(DictionaryTest
  DictionaryTest.cpp
  OldDictionary.cpp
  Stubs.cpp
  ${REPO_ROOT}/src/util/Dictionary.cpp
)
target_include_directories(DictionaryTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${CMAKE_CURRENT_SOURCE_DIR}
  ${REPO_ROOT}/src
  ${REPO_ROOT}/src/util
  ${REPO_ROOT}/lib/Memory
  ${REPO_ROOT}/lib/hal
)
target_link_libraries(DictionaryTest PRIVATE crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(DictionaryTest)
