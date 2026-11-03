#!/usr/bin/env bash
# Checks that driver.cpp, built over the Rust shim (src/util/BookmarkFile.cpp calling bembeyaz-ffi,
# with this tree's PersistableStore), answers every step of the corpus as the C++ BookmarkFile did.
# ArduinoJson's headers because PersistableStore and the driver (for the bookmark layers it prints)
# include them; the file's JSON is Rust's.
#   test/bookmark_file/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/bookmark_file" -DARDUINOJSON_ENABLE_ARDUINO_STRING=1 \
  -I"$root/lib/Serialization" -I"$root/src" -I"$root/src/util" "$root/test/json_oracle/stubs/ObfuscationFake.cpp" \
  "$root/src/util/BookmarkFile.cpp" "$root/src/util/BookmarkUtil.cpp" "$root/lib/Serialization/PersistableStore.cpp" \
  "$root/lib/Serialization/StoredPassword.cpp" "$root/lib/Serialization/StoreText.cpp"
