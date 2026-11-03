#!/usr/bin/env bash
# Builds driver.cpp over the Rust shim (lib/KOReaderSync/KOReaderSyncMessages.cpp calling
# bembeyaz-ffi) and checks it answers every step of the corpus as ArduinoJson did.
#   test/kosync_messages/check_shim.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
(cd "$root/rust" && cargo build --release --quiet -p bembeyaz-ffi --target-dir "$out/rust")
# ArduinoJson only for the driver's own `json` and `long` steps; the messages are Rust's.
c++ -std=c++17 -O1 -Wall -Wextra -isystem "$root/.pio/libdeps/x4pro/ArduinoJson/src" \
  -I"$root/lib/KOReaderSync" -I"$root/rust/include" "$root/test/kosync_messages/driver.cpp" \
  "$root/lib/KOReaderSync/KOReaderSyncMessages.cpp" "$out/rust/release/libbembeyaz_ffi.a" \
  -lpthread -ldl -lm -o "$out/driver"
python3 "$root/test/kosync_messages/make_corpus.py" --check "$out/driver"
