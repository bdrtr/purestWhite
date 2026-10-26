# purestwhite-epd's controller probe against freeink-sdk's XteinkDetect, on a model controller.
# The SDK's own host stubs declare the Arduino and GPIO calls; the test defines them.
set(SDK_HARDWARE ${REPO_ROOT}/freeink-sdk/libs/hardware)

add_executable(EpdProbeTest
  EpdProbeTest.cpp
  ${SDK_HARDWARE}/XteinkDetect/src/XteinkDetect.cpp
)

target_include_directories(EpdProbeTest PRIVATE
  ${SDK_HARDWARE}/XteinkDetect/test/host/stubs
  ${SDK_HARDWARE}/XteinkDetect/include
  ${SDK_HARDWARE}/BoardConfig/include
)

target_compile_definitions(EpdProbeTest PRIVATE FREEINK_DEVICE_X4PRO=1)

target_link_libraries(EpdProbeTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(EpdProbeTest)
