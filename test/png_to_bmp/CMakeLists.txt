add_executable(PngToBmpCorpusTest
  PngToBmpCorpusTest.cpp
  ${REPO_ROOT}/lib/PngToBmpConverter/PngToBmpConverter.cpp
  ${REPO_ROOT}/lib/GfxRenderer/BitmapHelpers.cpp
)
target_compile_definitions(PngToBmpCorpusTest PRIVATE
  PNG_CORPUS_DIR="${REPO_ROOT}/test/png_decoder/corpus"
  PNG_TO_BMP_CORPUS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/corpus"
)
target_include_directories(PngToBmpCorpusTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/lib/PngToBmpConverter
  ${REPO_ROOT}/lib/GfxRenderer
  ${REPO_ROOT}/lib/Memory
)
target_link_libraries(PngToBmpCorpusTest PRIVATE crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(PngToBmpCorpusTest)
