add_executable(DictZipTest
  DictZipTest.cpp
  ${REPO_ROOT}/src/util/DictZip.cpp
  ${REPO_ROOT}/lib/InflateStream/InflateStream.cpp
  ${REPO_ROOT}/lib/Memory/BuildScratch.cpp
  ${REPO_ROOT}/lib/hal/HalMemory.cpp
)
target_include_directories(DictZipTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/src
  ${REPO_ROOT}/lib/Memory
  ${REPO_ROOT}/lib/hal
  ${REPO_ROOT}/lib/InflateStream
)
target_link_libraries(DictZipTest PRIVATE crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(DictZipTest)
