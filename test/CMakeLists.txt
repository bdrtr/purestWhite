cmake_minimum_required(VERSION 3.16)
project(crosspoint_reader_tests C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

include(FetchContent)

FetchContent_Declare(
  googletest
  GIT_REPOSITORY https://github.com/google/googletest.git
  GIT_TAG v1.17.0
)

set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
set(BUILD_GMOCK OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(googletest)

set(REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/..")

# Rust modules (rust/) are linked into every test, as in the firmware. Cargo
# runs on each build and returns quickly when nothing changed.
find_program(CARGO cargo REQUIRED)
find_package(Threads REQUIRED)
set(BEMBEYAZ_RUST_TARGET_DIR "${CMAKE_CURRENT_BINARY_DIR}/rust")
set(BEMBEYAZ_RUST_LIB "${BEMBEYAZ_RUST_TARGET_DIR}/release/libbembeyaz_ffi.a")
add_custom_target(bembeyaz_rust_build
  COMMAND ${CARGO} build --release --quiet -p bembeyaz-ffi --target-dir ${BEMBEYAZ_RUST_TARGET_DIR}
  WORKING_DIRECTORY ${REPO_ROOT}/rust
  BYPRODUCTS ${BEMBEYAZ_RUST_LIB}
)
add_library(bembeyaz_rust INTERFACE)
add_dependencies(bembeyaz_rust bembeyaz_rust_build)
target_include_directories(bembeyaz_rust INTERFACE ${REPO_ROOT}/rust/include)
target_link_libraries(bembeyaz_rust INTERFACE ${BEMBEYAZ_RUST_LIB} Threads::Threads ${CMAKE_DL_LIBS} m)

# lib/InflateStream calls the ESP32-S3 ROM's tinfl_decompress (miniz 1.15); the tests that build
# it link the same version from source, built as the ROM was (rom_tinfl/miniz115_tinfl.c).
add_library(rom_tinfl STATIC rom_tinfl/miniz115_tinfl.c)
target_compile_options(rom_tinfl PRIVATE -w)

add_library(crosspoint_test_common INTERFACE)
target_include_directories(crosspoint_test_common INTERFACE
  ${REPO_ROOT}
  ${REPO_ROOT}/lib
)
target_link_libraries(crosspoint_test_common INTERFACE bembeyaz_rust)
target_compile_options(crosspoint_test_common INTERFACE
  -Wall
  -Wextra
  -pedantic
)

enable_testing()
include(GoogleTest)

add_subdirectory(streaming_json_parser)
add_subdirectory(json_sax_oracle)
add_subdirectory(release_json_parser)
add_subdirectory(differential_rounding)
add_subdirectory(hyphenation_eval)
add_subdirectory(utf8_compose)
add_subdirectory(opds_filename)
add_subdirectory(opds_feed_error)
add_subdirectory(html_to_plain_text)
add_subdirectory(library_text)
add_subdirectory(library_format)
add_subdirectory(library_index_file)
add_subdirectory(library_builder)
add_subdirectory(minibidi_arabic)
add_subdirectory(bidi_utils)
add_subdirectory(combining_marks)
add_subdirectory(credential_integrity)
add_subdirectory(xml_parser_utils)
add_subdirectory(sdk_xml_parsers)
add_subdirectory(koreader_xpath_resolver)
add_subdirectory(progress_comparison)
add_subdirectory(font_cache_manager)
add_subdirectory(css_parser)
add_subdirectory(chapter_html_slim_parser)
add_subdirectory(chapter_corpus)
add_subdirectory(gfx_primitives)
add_subdirectory(dither)
add_subdirectory(image_sampling)
add_subdirectory(image_probe)
add_subdirectory(dictionary)
add_subdirectory(dict_html)
add_subdirectory(korean_line_breaking)
add_subdirectory(em_dash_line_breaking)
add_subdirectory(ligature_guard)
add_subdirectory(page_link)
add_subdirectory(content_opf_parser)
add_subdirectory(toc_nav_parser)
add_subdirectory(toc_ncx_parser)
add_subdirectory(container_parser)
add_subdirectory(parsed_text)
add_subdirectory(png_decoder)
add_subdirectory(jpeg_decoder)
add_subdirectory(png_to_bmp)
add_subdirectory(bmp_reader)
add_subdirectory(xtc_parser)
add_subdirectory(chapter_position)
add_subdirectory(book_metadata_cache)
add_subdirectory(home_button)
add_subdirectory(button_press_navigation)

add_subdirectory(inflate_stream)
add_subdirectory(dict_zip)
add_subdirectory(font_decompressor)
add_subdirectory(zip_file)
add_subdirectory(fs_helpers)
add_subdirectory(http_header)
add_subdirectory(tls_policy)
add_subdirectory(ota_version)
add_subdirectory(secure_net)
add_subdirectory(txt_to_html)

add_subdirectory(absolute_grayscale)

add_subdirectory(sd_card_font)
add_subdirectory(section_file)
add_subdirectory(page_codec)
add_subdirectory(clipping_preview)
add_subdirectory(glyph_bitmap)

add_subdirectory(clipping_store)
add_subdirectory(reader_session)
add_subdirectory(plugin_manifest_classification)
add_subdirectory(plugin_picker_layout)
add_subdirectory(protected_paths)
add_subdirectory(web_paths)
add_subdirectory(rtc_pcf8563)
add_subdirectory(cw2017_gauge)

find_package(Python3 REQUIRED COMPONENTS Interpreter)
add_test(NAME TrustedTimeTimezone COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/test_trusted_time.py)
add_test(NAME RemoteButtonInputIntegration COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/test_remote_button_input.py)
add_test(NAME BleLearning COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/test_ble_learning.py)
add_subdirectory(ble_link_note_title)
add_subdirectory(ble_chapter_build_gate)
add_subdirectory(ble_remote_turn_gate)

add_subdirectory(${REPO_ROOT}/lib/BlePageTurner/test BlePageTurnerTests)

add_test(NAME selection_buttons COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/test_selection_buttons.py)
add_test(NAME list_viewport COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/test_list_viewport.py)
add_test(NAME clipping_highlight_planes COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/test_clipping_highlight_planes.py)
add_test(NAME clipping_selection_text COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/test_clipping_selection_text.py)
set_tests_properties(clipping_selection_text PROPERTIES ENVIRONMENT "BEMBEYAZ_RUST_LIB=${BEMBEYAZ_RUST_LIB}")
