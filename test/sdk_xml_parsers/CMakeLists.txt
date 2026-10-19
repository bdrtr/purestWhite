set(SDK_NETWORK ${REPO_ROOT}/freeink-sdk/libs/network)

add_executable(SdkXmlParsersTest
  SdkXmlParsersTest.cpp
  ${SDK_NETWORK}/Opds/src/OpdsParser.cpp
  ${SDK_NETWORK}/Opds/src/OpenSearchDescParser.cpp
  ${SDK_NETWORK}/CatalogList/src/XmlListParser.cpp
)

target_include_directories(SdkXmlParsersTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${SDK_NETWORK}/Opds/include
  ${SDK_NETWORK}/CatalogList/include
)

target_link_libraries(SdkXmlParsersTest PRIVATE
  crosspoint_test_common
  crosspoint_expat
  GTest::gtest_main
)

gtest_discover_tests(SdkXmlParsersTest)
