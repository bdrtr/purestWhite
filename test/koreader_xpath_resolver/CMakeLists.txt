add_executable(KOReaderXPathResolverTest
  KOReaderXPathResolverTest.cpp
  ${REPO_ROOT}/lib/KOReaderSync/ChapterXPathResolver.cpp
)

target_include_directories(KOReaderXPathResolverTest PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/stubs
  ${REPO_ROOT}/lib/KOReaderSync
)

target_link_libraries(KOReaderXPathResolverTest PRIVATE
  crosspoint_test_common
  GTest::gtest_main
)

gtest_discover_tests(KOReaderXPathResolverTest)
