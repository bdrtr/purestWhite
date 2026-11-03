// Pins compareProgress and selectRemoteRecord over every combination of the evidence flags, two
// values of each field and percentages on both sides of the 0.001 tolerance (and NaN, infinities),
// as a hash of the answers: the oracle for the Rust port.
#include <gtest/gtest.h>

#include <cstdint>
#include <limits>
#include <vector>

#include "ProgressComparison.h"

namespace {
std::vector<CrossPointPosition> positions() {
  std::vector<CrossPointPosition> out;
  for (int flags = 0; flags < 8; ++flags) {
    for (const int spine : {2, 3}) {
      for (const int page : {4, 5}) {
        for (const uint32_t offset : {10u, 11u}) {
          CrossPointPosition p{};
          p.spineIndex = spine;
          p.pageNumber = page;
          p.totalPages = 20;
          p.visibleTextOffset = offset;
          p.hasResolvedSpineIndex = flags & 1;
          p.hasMappedPage = flags & 2;
          p.hasVisibleTextOffset = flags & 4;
          out.push_back(p);
        }
      }
    }
  }
  return out;
}

const float kPercentages[] = {0.5f,
                              0.5009f,
                              0.501f,
                              0.5011f,
                              0.50105f,
                              0.49895f,
                              0.4991f,
                              0.499f,
                              0.0f,
                              -0.0f,
                              1.0f,
                              std::numeric_limits<float>::quiet_NaN(),
                              std::numeric_limits<float>::infinity(),
                              -std::numeric_limits<float>::infinity()};

uint64_t mix(uint64_t h, const uint8_t v) { return (h ^ v) * 1099511628211ull; }
}  // namespace

TEST(ProgressComparisonCorpus, EveryCombinationAnswersAsPinned) {
  const std::vector<CrossPointPosition> all = positions();
  uint64_t compare = 14695981039346656037ull;
  uint64_t select = 14695981039346656037ull;
  size_t count = 0;
  for (const CrossPointPosition& local : all) {
    for (const CrossPointPosition& remote : all) {
      for (const float lp : kPercentages) {
        for (const float rp : kPercentages) {
          compare = mix(compare, static_cast<uint8_t>(compareProgress(local, lp, remote, rp)));
          select = mix(select, static_cast<uint8_t>(selectRemoteRecord(local, lp, remote, rp)));
          ++count;
        }
      }
    }
  }
  EXPECT_EQ(count, 64u * 64u * 196u);
  EXPECT_EQ(compare, 4743575666551028741ull) << compare;
  EXPECT_EQ(select, 6755756443072042101ull) << select;
}
