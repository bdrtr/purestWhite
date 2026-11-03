#include "ProgressComparison.h"

#include <bembeyaz/kosync.h>

#include <cstddef>

// The comparison is Rust's (bembeyaz_kosync::progress); this hands it the fields it reads.
namespace {
bb_kosync_position positionOf(const CrossPointPosition& p) {
  return bb_kosync_position{p.spineIndex,    p.pageNumber,          p.visibleTextOffset, p.hasResolvedSpineIndex,
                            p.hasMappedPage, p.hasVisibleTextOffset};
}

static_assert(sizeof(bb_kosync_position) == 16 && offsetof(bb_kosync_position, page) == 4 &&
                  offsetof(bb_kosync_position, offset) == 8 && offsetof(bb_kosync_position, spine_resolved) == 12 &&
                  offsetof(bb_kosync_position, page_mapped) == 13 && offsetof(bb_kosync_position, has_offset) == 14,
              "bembeyaz_kosync::progress::Position");
static_assert(static_cast<uint8_t>(ProgressComparison::LocalAhead) == BB_KOSYNC_LOCAL_AHEAD &&
                  static_cast<uint8_t>(ProgressComparison::Synchronized) == BB_KOSYNC_SYNCHRONIZED &&
                  static_cast<uint8_t>(ProgressComparison::RemoteAhead) == BB_KOSYNC_REMOTE_AHEAD &&
                  static_cast<uint8_t>(ProgressComparison::Unknown) == BB_KOSYNC_UNKNOWN,
              "kosync.h's comparison answers");
}  // namespace

ProgressComparison compareProgress(const CrossPointPosition& local, const float localPercentage,
                                   const CrossPointPosition& remote, const float remotePercentage) {
  const bb_kosync_position l = positionOf(local);
  const bb_kosync_position r = positionOf(remote);
  return static_cast<ProgressComparison>(bb_kosync_compare_progress(&l, localPercentage, &r, remotePercentage));
}

RemoteRecordChoice selectRemoteRecord(const CrossPointPosition& primary, const float primaryPercentage,
                                      const CrossPointPosition& alternate, const float alternatePercentage) {
  const bb_kosync_position p = positionOf(primary);
  const bb_kosync_position a = positionOf(alternate);
  return bb_kosync_select_remote(&p, primaryPercentage, &a, alternatePercentage) ? RemoteRecordChoice::Alternate
                                                                                 : RemoteRecordChoice::Primary;
}
