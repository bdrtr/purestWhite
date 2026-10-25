# purestwhite-gauge (through lib/hal/WireBus.h) against freeink-sdk's CW2017 path in
# BatteryMonitor, which it replaced, on the SDK's recording Wire stub.
set(SDK_HARDWARE ${REPO_ROOT}/freeink-sdk/libs/hardware)

add_executable(Cw2017GaugeTest
  Cw2017GaugeTest.cpp
  ${SDK_HARDWARE}/BatteryMonitor/src/BatteryMonitor.cpp
)

target_include_directories(Cw2017GaugeTest PRIVATE
  ${SDK_HARDWARE}/BatteryMonitor/test/host/stubs
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${SDK_HARDWARE}/BatteryMonitor/include
  ${SDK_HARDWARE}/BoardConfig/include
  ${SDK_HARDWARE}/XteinkDetect/test/host/stubs
  ${REPO_ROOT}/lib/hal
)

target_compile_definitions(Cw2017GaugeTest PRIVATE FREEINK_DEVICE_X4PRO=1)
# BoardConfig.h uses a braced list without including <initializer_list> (the SDK's own tests get it
# through earlier headers).
target_compile_options(Cw2017GaugeTest PRIVATE -include initializer_list)

target_link_libraries(Cw2017GaugeTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(Cw2017GaugeTest)
