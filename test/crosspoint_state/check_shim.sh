#!/usr/bin/env bash
# Checks that driver.cpp, built with -DSHIM over the Rust shim (src/CrossPointState.cpp calling
# bembeyaz-ffi), answers every step of the corpus as the C++ store did. ArduinoJson's headers only
# because PersistableStore.h and driver.cpp include them; the store's JSON is Rust's.
#   test/crosspoint_state/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/crosspoint_state" -DSHIM \
  -I"$root/lib/Serialization" -I"$root/src" \
  "$root/src/CrossPointState.cpp"
