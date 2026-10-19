# The JPEG decoder the firmware uses for chapter images and covers, as the oracle the Rust decoder
# is checked against: JPEGDEC at the commit platformio.ini pins, with the patches
# scripts/patch_jpegdec.py applies, built as the firmware builds it (a 32-bit bit buffer, no
# unaligned reads, no SIMD: ESP_PLATFORM leaves HAL_ESP32_HAL_H_ undefined there).
FetchContent_Declare(
  jpegdec
  GIT_REPOSITORY https://github.com/bitbank2/JPEGDEC.git
  GIT_TAG 86282979224c8a32fd51e091ed5a35b0c699a52b
  # Reset first: a reconfigure runs the patch step again.
  PATCH_COMMAND git checkout -- . COMMAND git apply
    ${REPO_ROOT}/scripts/jpegdec_patches/0001-redirect-pmcu-on-mcu-skip.patch
    ${REPO_ROOT}/scripts/jpegdec_patches/0002-guard-dc-writes-on-mcu-skip.patch
    ${REPO_ROOT}/scripts/jpegdec_patches/0003-fix-skip-absent-progressive-chroma-scans.patch
  # Its CMakeLists.txt is an ESP-IDF component: fetch only.
  SOURCE_SUBDIR none
)
FetchContent_MakeAvailable(jpegdec)

set(JPEGDEC_DEVICE_SRC ${CMAKE_CURRENT_BINARY_DIR}/jpegdec)
file(COPY ${jpegdec_SOURCE_DIR}/src/JPEGDEC.cpp ${jpegdec_SOURCE_DIR}/src/jpeg.inl DESTINATION ${JPEGDEC_DEVICE_SRC})
file(READ ${jpegdec_SOURCE_DIR}/src/JPEGDEC.h JPEGDEC_HEADER)
function(jpegdec_header_replace from to)
  string(FIND "${JPEGDEC_HEADER}" "${from}" at)
  if(at EQUAL -1)
    message(FATAL_ERROR "JPEGDEC.h changed: cannot find \"${from}\"")
  endif()
  string(REPLACE "${from}" "${to}" replaced "${JPEGDEC_HEADER}")
  set(JPEGDEC_HEADER "${replaced}" PARENT_SCOPE)
endfunction()
jpegdec_header_replace("|| defined (__x86_64__) || defined(TEENSYDUINO)\n#define ALLOWS_UNALIGNED"
                       "|| defined(TEENSYDUINO)\n#define ALLOWS_UNALIGNED")
jpegdec_header_replace("defined(__aarch64__) || defined (__x86_64__)\n#define REGISTER_WIDTH 64"
                       "defined(__aarch64__)\n#define REGISTER_WIDTH 64")
file(WRITE ${JPEGDEC_DEVICE_SRC}/JPEGDEC.h "${JPEGDEC_HEADER}")

add_library(jpegdec_oracle STATIC ${JPEGDEC_DEVICE_SRC}/JPEGDEC.cpp)
target_include_directories(jpegdec_oracle PUBLIC ${JPEGDEC_DEVICE_SRC})
target_compile_definitions(jpegdec_oracle PUBLIC __LINUX__ NO_SIMD)
target_compile_options(jpegdec_oracle PRIVATE -w)

add_executable(JpegDecoderCorpusTest JpegDecoderCorpusTest.cpp)
target_compile_definitions(JpegDecoderCorpusTest PRIVATE JPEG_CORPUS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/corpus")
target_link_libraries(JpegDecoderCorpusTest PRIVATE jpegdec_oracle crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(JpegDecoderCorpusTest)
