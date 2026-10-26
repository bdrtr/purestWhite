# purestwhite-epd's UC8279 and UC8179 against freeink-sdk's EpdBus and its drivers, on a model controller
# behind our own Arduino and SPI stubs.
set(SDK_DISPLAY ${REPO_ROOT}/freeink-sdk/libs/display/FreeInkDisplay)

add_executable(EpdPanelsTest
  EpdPanelsTest.cpp
  ${SDK_DISPLAY}/src/bus/EpdBus.cpp
  ${SDK_DISPLAY}/src/driver/Uc8179Driver.cpp
  ${SDK_DISPLAY}/src/driver/Uc8279X4Driver.cpp
)

target_include_directories(EpdPanelsTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${SDK_DISPLAY}/src
  ${SDK_DISPLAY}/include
  ${REPO_ROOT}/freeink-sdk/libs/hardware/BoardConfig/include
)

target_compile_definitions(EpdPanelsTest PRIVATE FREEINK_DEVICE_X4PRO=1)
# BoardConfig.h uses a braced list without including <initializer_list>.
target_compile_options(EpdPanelsTest PRIVATE -include initializer_list)

target_link_libraries(EpdPanelsTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(EpdPanelsTest)
