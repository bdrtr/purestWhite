add_executable(TocNcxParserTest
  TocNcxParserTest.cpp
  ${REPO_ROOT}/lib/Epub/Epub/parsers/TocParser.cpp
)

target_include_directories(TocNcxParserTest PRIVATE
  ${REPO_ROOT}/test/toc_nav_parser/stubs
  ${REPO_ROOT}/lib/Epub
  ${REPO_ROOT}/lib/Epub/Epub/parsers
)

target_link_libraries(TocNcxParserTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(TocNcxParserTest)
