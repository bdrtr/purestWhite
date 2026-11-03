#!/usr/bin/env bash
# Builds TEST_DIR/driver.cpp over a C++ shim and bembeyaz-ffi (with ArduinoJson's headers and
# rust/include on the include path, then the extra compiler arguments), and checks that it answers
# every step of TEST_DIR's corpus as the C++ did (TEST_DIR/make_corpus.py --check).
#   test/json_oracle/check_shim.sh TEST_DIR [C++ ARGUMENTS: flags, include paths, shim sources]
# Needs cargo, a host c++, and ArduinoJson under .pio/libdeps/x4pro (after `pio run -e x4pro`).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
test_dir="$1"
shift
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
(cd "$root/rust" && cargo build --release --quiet -p bembeyaz-ffi --target-dir "$out/rust")
c++ -std=c++17 -O1 -Wall -Wextra -isystem "$root/.pio/libdeps/x4pro/ArduinoJson/src" \
  -I"$root/rust/include" "$@" "$test_dir/driver.cpp" "$out/rust/release/libbembeyaz_ffi.a" \
  -lpthread -ldl -lm -o "$out/driver"
python3 "$test_dir/make_corpus.py" --check "$out/driver"
