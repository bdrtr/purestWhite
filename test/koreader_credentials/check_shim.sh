#!/usr/bin/env bash
# Checks that driver.cpp, built over the Rust shim (lib/KOReaderSync/KOReaderCredentialStore.cpp
# calling bembeyaz-ffi, with this tree's PersistableStore), answers every step of
# the corpus as the C++ store did. ArduinoJson's headers because PersistableStore and the driver
# (for the stored-text layers it prints) include them; the store's JSON is Rust's.
#   test/koreader_credentials/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/koreader_credentials" \
  -DARDUINOJSON_ENABLE_ARDUINO_STRING=1 -I"$root/lib/Serialization" -I"$root/lib/HttpHeader" \
  -I"$root/lib/KOReaderSync" "$root/test/json_oracle/stubs/ObfuscationFake.cpp" \
  "$root/lib/KOReaderSync/KOReaderCredentialStore.cpp" "$root/lib/Serialization/PersistableStore.cpp" \
  "$root/lib/Serialization/StoreText.cpp"
