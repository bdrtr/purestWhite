add_executable(StreamingJsonParserTest
  StreamingJsonParserTest.cpp
  ${REPO_ROOT}/lib/JsonSax/src/StreamingJsonParser.cpp
)

target_include_directories(StreamingJsonParserTest PRIVATE
  ${REPO_ROOT}/lib/JsonParser
  ${REPO_ROOT}/lib/JsonSax/include
)

target_link_libraries(StreamingJsonParserTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(StreamingJsonParserTest)
