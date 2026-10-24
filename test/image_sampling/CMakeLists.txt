add_executable(ImageSamplingTest ImageSamplingTest.cpp)
target_link_libraries(ImageSamplingTest PRIVATE crosspoint_test_common GTest::gtest_main)
# The old writer's grayPlanePixel.
target_include_directories(ImageSamplingTest PRIVATE ${REPO_ROOT}/lib/Memory ${REPO_ROOT}/lib/GfxRenderer)
gtest_discover_tests(ImageSamplingTest)
# The old writer's orientation switch has no default (GCC cannot see that it covers every value).
target_compile_options(ImageSamplingTest PRIVATE -Wno-maybe-uninitialized -Wno-unused-parameter)
