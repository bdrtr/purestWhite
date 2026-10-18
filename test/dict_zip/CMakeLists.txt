add_executable(DictZipTest
  DictZipTest.cpp
  ${REPO_ROOT}/src/util/DictZip.cpp
  ${REPO_ROOT}/lib/InflateReader/InflateReader.cpp
  ${REPO_ROOT}/lib/uzlib/src/tinflate.c
)
target_include_directories(DictZipTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/src
  ${REPO_ROOT}/lib/Memory
  ${REPO_ROOT}/lib/InflateReader
  ${REPO_ROOT}/lib/uzlib/src
)
target_link_libraries(DictZipTest PRIVATE crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(DictZipTest)
# The vendored uzlib leaves out the checksum helpers only uzlib_uncompress_chksum uses.
target_compile_options(DictZipTest PRIVATE -ffunction-sections)
target_link_options(DictZipTest PRIVATE -Wl,--gc-sections)
