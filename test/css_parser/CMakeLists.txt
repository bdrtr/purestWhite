add_executable(CssParserTest
  CssParserTest.cpp
  CssCorpusTest.cpp
  CssImageSizeTest.cpp
  ${REPO_ROOT}/lib/Epub/Epub/css/CssParser.cpp
)

target_include_directories(CssParserTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/lib/Epub/Epub/css
)

target_link_libraries(CssParserTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(CssParserTest)
