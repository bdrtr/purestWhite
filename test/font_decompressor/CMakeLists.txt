add_executable(FontDecompressorTest
  FontDecompressorTest.cpp
  ${REPO_ROOT}/lib/EpdFont/FontDecompressor.cpp
  ${REPO_ROOT}/lib/InflateStream/InflateStream.cpp
  ${REPO_ROOT}/lib/Memory/BuildScratch.cpp
  ${REPO_ROOT}/lib/hal/HalMemory.cpp
)
target_include_directories(FontDecompressorTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/lib/EpdFont
  ${REPO_ROOT}/lib/Memory
  ${REPO_ROOT}/lib/hal
  ${REPO_ROOT}/lib/InflateStream
)
target_link_libraries(FontDecompressorTest PRIVATE crosspoint_test_common rom_tinfl GTest::gtest_main)
gtest_discover_tests(FontDecompressorTest)
