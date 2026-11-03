#!/usr/bin/env bash
# Checks that driver.cpp over the Rust shim (lib/KOReaderSync/KOReaderSyncMessages.cpp calling
# bembeyaz-ffi) answers every step of the corpus as ArduinoJson did. ArduinoJson only for the
# driver's own `json` and `long` steps; the messages are Rust's.
#   test/kosync_messages/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
exec "$root/test/json_oracle/check_shim.sh" "$root/test/kosync_messages" -I"$root/lib/KOReaderSync" \
  "$root/lib/KOReaderSync/KOReaderSyncMessages.cpp"
