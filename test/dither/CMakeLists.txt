add_executable(DitherTest DitherTest.cpp)
target_link_libraries(DitherTest PRIVATE crosspoint_test_common GTest::gtest_main)
# OldBitmapHelpers.h's Memory.h.
target_include_directories(DitherTest PRIVATE ${REPO_ROOT}/lib/Memory)
gtest_discover_tests(DitherTest)
