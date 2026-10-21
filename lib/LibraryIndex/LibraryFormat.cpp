#include "LibraryFormat.h"

namespace library {

// ClixValidity's numbers are format::Validity's, which bb_library_validate_header returns.
static_assert(static_cast<int>(ClixValidity::BadMagic) == 1 && static_cast<int>(ClixValidity::StaleFoldVersion) == 3 &&
                  static_cast<int>(ClixValidity::SectionsInconsistent) == 6,
              "bb_library_validate_header results");

const char* clixValidityName(const ClixValidity v) {
  switch (v) {
    case ClixValidity::Ok:
      return "ok";
    case ClixValidity::BadMagic:
      return "bad magic";
    case ClixValidity::UnknownFormatVersion:
      return "unknown format version";
    case ClixValidity::StaleFoldVersion:
      return "stale fold version";
    case ClixValidity::SizeMismatch:
      return "size mismatch (truncated?)";
    case ClixValidity::CountOutOfRange:
      return "book count out of range";
    case ClixValidity::SectionsInconsistent:
      return "section offsets inconsistent";
  }
  return "unknown";
}

}  // namespace library
