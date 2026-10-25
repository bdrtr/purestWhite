# purestwhite-rtc (through lib/hal/WireBus.h) against freeink-sdk's Rtc driver, which it replaced,
# on the SDK's recording Wire stub.
set(SDK_HARDWARE ${REPO_ROOT}/freeink-sdk/libs/hardware)

add_executable(RtcPcf8563Test
  RtcPcf8563Test.cpp
  ${SDK_HARDWARE}/Rtc/src/Rtc.cpp
)

target_include_directories(RtcPcf8563Test PRIVATE
  ${SDK_HARDWARE}/Rtc/test/host/stubs
  ${SDK_HARDWARE}/BatteryMonitor/test/host/stubs
  ${SDK_HARDWARE}/Rtc/include
  ${REPO_ROOT}/lib/hal
)

target_compile_definitions(RtcPcf8563Test PRIVATE FREEINK_CAP_RTC=1)

target_link_libraries(RtcPcf8563Test PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(RtcPcf8563Test)
