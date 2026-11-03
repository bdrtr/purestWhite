#!/usr/bin/env bash
# Checks that driver.cpp, built over the Rust shim (src/WifiCredentialStore.cpp calling
# bembeyaz-ffi, with this tree's PersistableStore and StoredPassword), answers every step of the
# corpus as the C++ store did. ArduinoJson's headers because PersistableStore and the driver (for
# the stored-text layers it prints) include them; the store's JSON is Rust's.
#   test/wifi_credentials/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/wifi_credentials" -DARDUINOJSON_ENABLE_ARDUINO_STRING=1 \
  -DSTORE_LOG_ERRORS \
  -I"$root/lib/Serialization" -I"$root/src" \
  "$root/test/json_oracle/stubs/ObfuscationFake.cpp" "$root/src/WifiCredentialStore.cpp" \
  "$root/lib/Serialization/PersistableStore.cpp" "$root/lib/Serialization/StoredPassword.cpp" \
  "$root/lib/Serialization/StoreText.cpp"
