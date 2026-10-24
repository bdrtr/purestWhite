// The parts of the dictionary reader this test leaves out: every folder is /d/<folder>/dict, and a
// .dict.dz entry always fails to read (only plain .dict data is compared).
#include <string>

#include "util/DictZip.h"
#include "util/DictionaryRegistry.h"

namespace DictionaryRegistry {
bool resolveBasePath(const char* folderName, std::string& basePathOut) {
  basePathOut = std::string("/d/") + folderName + "/dict";
  return true;
}
}  // namespace DictionaryRegistry

namespace DictZip {
bool extractEntry(const char*, uint32_t, uint32_t, HalFile&, ExtractError* outError) {
  if (outError) *outError = ExtractError::ReadError;
  return false;
}
}  // namespace DictZip
