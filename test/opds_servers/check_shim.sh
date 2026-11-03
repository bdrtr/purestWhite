#!/usr/bin/env bash
# Checks that driver.cpp, built over the Rust shim (src/OpdsServerStore.cpp calling
# bembeyaz-ffi, with this tree's PersistableStore), answers every step of the corpus as the C++
# store did. ArduinoJson's headers only because PersistableStore includes them; the store's JSON is
# Rust's.
#   test/opds_servers/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/opds_servers" -DARDUINOJSON_ENABLE_ARDUINO_STRING=1 \
  -I"$root/lib/Serialization" -I"$root/lib/HttpHeader" \
  -I"$root/src" \
  "$root/test/json_oracle/stubs/ObfuscationFake.cpp" "$root/src/OpdsServerStore.cpp" \
  "$root/lib/Serialization/PersistableStore.cpp" "$root/lib/Serialization/StoredPassword.cpp" \
  "$root/lib/Serialization/StoreText.cpp"
