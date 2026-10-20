add_executable(ContainerParserTest
  ContainerParserTest.cpp
  ${REPO_ROOT}/lib/Epub/Epub/parsers/ContainerParser.cpp
)

target_include_directories(ContainerParserTest PRIVATE
  ${REPO_ROOT}/test/toc_nav_parser/stubs
  ${REPO_ROOT}/lib/Epub/Epub/parsers
  ${REPO_ROOT}/lib/XmlParserUtils
)

target_link_libraries(ContainerParserTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(ContainerParserTest)
