#include "LibraryText.h"

#include <bembeyaz/library.h>

#include "LibraryFormat.h"

namespace library {

// An author key fills the index record's field.
static_assert(AUTHOR_KEY_MAX_BYTES == CLIX_AUTHOR_KEY_BYTES, "author key width");

std::string joinLibraryPath(const std::string_view folder, const std::string_view name) {
  std::string path;
  path.reserve(folder.size() + name.size() + 1);
  if (folder.empty()) {
    path.push_back('/');
  } else {
    path.append(folder);
    if (folder.back() != '/') path.push_back('/');
  }
  path.append(name);
  return path;
}

namespace {
// Runs one of the bembeyaz-library text functions; empty when out of memory.
std::string text(bool (*f)(const char*, size_t, void (*)(void*, const char*, size_t), void*),
                 const std::string_view in) {
  std::string result;
  const auto assign = [](void* context, const char* s, const size_t len) {
    static_cast<std::string*>(context)->assign(s, len);
  };
  f(in.data(), in.size(), assign, &result);
  return result;
}
}  // namespace

std::string fold(const std::string_view text) { return library::text(bb_library_fold, text); }

uint32_t foldedGroupInitial(const std::string_view folded) {
  return bb_library_folded_group_initial(folded.data(), folded.size());
}

std::string cleanPersonName(const std::string_view author) {
  return library::text(bb_library_clean_person_name, author);
}

std::string authorKey(const std::string_view author) { return library::text(bb_library_author_key, author); }

bool matchesQuery(const std::string_view haystack, const std::string_view needle) {
  return bb_library_matches_query(haystack.data(), haystack.size(), needle.data(), needle.size());
}

std::string surnameKey(const std::string_view displayAuthor) {
  return library::text(bb_library_surname_key, displayAuthor);
}

}  // namespace library
