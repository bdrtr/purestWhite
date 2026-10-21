add_executable(BmpReaderTest
  BmpReaderTest.cpp
  ${REPO_ROOT}/lib/GfxRenderer/Bitmap.cpp
  ${REPO_ROOT}/lib/GfxRenderer/BitmapHelpers.cpp
)
target_include_directories(BmpReaderTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/lib/GfxRenderer
  ${REPO_ROOT}/lib/Memory
)
target_link_libraries(BmpReaderTest PRIVATE crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(BmpReaderTest)
