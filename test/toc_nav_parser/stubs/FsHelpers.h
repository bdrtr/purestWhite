#pragma once

#include <bembeyaz/path.h>

#include <string>

// The two path helpers the TOC parsers use, as lib/FsHelpers implements them (rust/crates/bembeyaz-path).
namespace FsHelpers {

inline std::string normalisePath(const std::string& path) {
  std::string normalised = path;
  normalised.resize(bb_path_normalise(normalised.data(), normalised.size()));
  return normalised;
}

inline std::string decodeUriEscapes(const std::string& path) {
  std::string decoded = path;
  decoded.resize(bb_path_decode_uri_escapes(decoded.data(), decoded.size()));
  return decoded;
}

}  // namespace FsHelpers
