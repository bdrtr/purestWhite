# Differential test: the SDK's original C++ StreamingJsonParser, renamed by macros, against this
# fork's Rust one (lib/JsonSax) on generated and corrupted documents fed in random pieces.
set(SDK_JSON_SAX ${REPO_ROOT}/freeink-sdk/libs/network/JsonSax)

add_library(SdkJsonSaxOracle STATIC ${SDK_JSON_SAX}/src/StreamingJsonParser.cpp)
target_include_directories(SdkJsonSaxOracle PRIVATE ${SDK_JSON_SAX}/include)
target_compile_definitions(SdkJsonSaxOracle PRIVATE StreamingJsonParser=SdkStreamingJsonParser
                                                    JsonCallbacks=SdkJsonCallbacks)

add_executable(JsonSaxOracleTest
  JsonSaxOracleTest.cpp
  ${REPO_ROOT}/lib/JsonSax/src/StreamingJsonParser.cpp
)

target_include_directories(JsonSaxOracleTest PRIVATE ${REPO_ROOT}/lib/JsonSax/include)

target_link_libraries(JsonSaxOracleTest PRIVATE
  SdkJsonSaxOracle
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(JsonSaxOracleTest)
