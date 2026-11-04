#!/usr/bin/env bash
# Checks that driver.cpp, built over the Rust shims (src/OpdsTokenStore.cpp,
# src/LcpPassphraseStore.cpp and src/network/LcpDeviceStore.cpp calling bembeyaz-ffi, with this
# tree's PersistableStore), answers every step of the corpus as the C++ stores
# did. ArduinoJson's headers because PersistableStore and the driver (for the record layers it
# prints) include them; the stores' JSON is Rust's.
#   test/token_stores/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/token_stores" -DARDUINOJSON_ENABLE_ARDUINO_STRING=1 \
  -I"$root/lib/Serialization" -I"$root/src" "$root/test/json_oracle/stubs/ObfuscationFake.cpp" \
  "$root/src/OpdsTokenStore.cpp" "$root/src/LcpPassphraseStore.cpp" "$root/src/network/LcpDeviceStore.cpp" \
  "$root/lib/Serialization/PersistableStore.cpp" \
  "$root/lib/Serialization/StoreText.cpp"
