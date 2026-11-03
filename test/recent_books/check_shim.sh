#!/usr/bin/env bash
# Checks that driver.cpp, built with -DSHIM over the Rust shim (src/RecentBooksStore.cpp calling
# bembeyaz-ffi), answers every step of the corpus as the C++ store did. ArduinoJson's headers only
# because PersistableStore.h and driver.cpp include them; the store's JSON is Rust's.
#   test/recent_books/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/recent_books" -DSHIM \
  -I"$root/test/recent_books/stubs" -I"$root/lib/Serialization" -I"$root/src" \
  "$root/src/RecentBooksStore.cpp"
