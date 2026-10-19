add_executable(XmlParserUtilsTest
  XmlParserUtilsTest.cpp
  XmlCorpusTest.cpp
)

target_include_directories(XmlParserUtilsTest PRIVATE
  ${REPO_ROOT}/lib/XmlParserUtils
)

target_link_libraries(XmlParserUtilsTest PRIVATE
  crosspoint_test_common
  crosspoint_expat
  GTest::gtest_main
)

gtest_discover_tests(XmlParserUtilsTest)
