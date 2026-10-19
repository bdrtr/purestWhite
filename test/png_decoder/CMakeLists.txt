# The PNG decoder the firmware used for chapter images, as the oracle the Rust decoder is checked
# against (pinned to the version platformio.ini pinned).
FetchContent_Declare(
  pngdec
  GIT_REPOSITORY https://github.com/bitbank2/PNGdec.git
  GIT_TAG 9a9c585fd39d148c5517597b02ce490e0fdb1bb4  # 1.1.6
)
FetchContent_MakeAvailable(pngdec)
file(GLOB PNGDEC_SOURCES ${pngdec_SOURCE_DIR}/src/*.c ${pngdec_SOURCE_DIR}/src/PNGdec.cpp)
add_library(pngdec_oracle STATIC ${PNGDEC_SOURCES})
target_include_directories(pngdec_oracle PUBLIC ${pngdec_SOURCE_DIR}/src)
target_compile_definitions(pngdec_oracle PUBLIC __LINUX__ PNG_MAX_BUFFERED_PIXELS=16416)
target_compile_options(pngdec_oracle PRIVATE -w)

add_executable(PngDecoderCorpusTest PngDecoderCorpusTest.cpp)
target_compile_definitions(PngDecoderCorpusTest PRIVATE PNG_CORPUS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/corpus")
target_link_libraries(PngDecoderCorpusTest PRIVATE pngdec_oracle crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(PngDecoderCorpusTest)
