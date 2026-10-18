#!/usr/bin/env bash
# Runs a scenario on the unmodified CrossPoint build (BASE) and on this tree's
# build side by side in eink-qemu, and prints the comparison table.
#   emu/compare.sh [scenario.txt] [env]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
scenario="${1:-$here/reading.txt}"
env="${2:-x4pro}"
qemu="${EINK_QEMU:-$root/../tools/eink-qemu-0.7.0/qemu/bin/eink-qemu}"
base="${BASE:-$root/../crosspoint-base}/.pio/build/$env"
out="$here/runs/$(basename "$scenario" .txt)-$(date +%Y%m%d-%H%M%S)"

mkdir -p "$here/books" "$here/runs"
python3 "$here/make_utf8_epub.py" "$here/books" >/dev/null
puts=()
for book in "$here"/books/*; do puts+=(--put "$book"); done

"$qemu" compare "$base" "$root/.pio/build/$env" "${puts[@]}" --out "$out" --script "$scenario" >"$out.jsonl"
sed -n '1,/^<details>/p' "$out/compare.md"
echo "Report: $out/report.html"
