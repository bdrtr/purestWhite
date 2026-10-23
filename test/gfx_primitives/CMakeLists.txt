add_executable(GfxPrimitivesTest GfxPrimitivesTest.cpp)
target_link_libraries(GfxPrimitivesTest PRIVATE crosspoint_test_common GTest::gtest_main)
gtest_discover_tests(GfxPrimitivesTest)
# The old code's orientation switch has no default (GCC cannot see that it covers every value), and
# its Clear dither ignores the pixel.
target_compile_options(GfxPrimitivesTest PRIVATE -Wno-maybe-uninitialized -Wno-unused-parameter)
