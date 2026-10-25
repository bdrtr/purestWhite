# purestwhite-gt911 (through lib/hal/WireBus.h) against freeink-sdk's InputManager GT911 path, on
# a model controller behind our own Arduino and Wire stubs.
set(SDK_HARDWARE ${REPO_ROOT}/freeink-sdk/libs/hardware)

add_executable(Gt911TouchTest
  Gt911TouchTest.cpp
  ${SDK_HARDWARE}/InputManager/src/InputManager.cpp
)

target_include_directories(Gt911TouchTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  # FreeRTOS and driver/gpio.h only; our stubs come first for Arduino.h and Wire.h.
  ${SDK_HARDWARE}/InputManager/test/host/metalio_stubs
  ${SDK_HARDWARE}/InputManager/include
  ${SDK_HARDWARE}/BoardConfig/include
  ${REPO_ROOT}/lib/hal
)

target_compile_definitions(Gt911TouchTest PRIVATE FREEINK_DEVICE_X4PRO=1)

target_link_libraries(Gt911TouchTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(Gt911TouchTest)
