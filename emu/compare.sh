#!/usr/bin/env bash
# Runs a scenario on the unmodified CrossPoint build (BASE) and on this tree's
# build side by side in eink-qemu, and prints the comparison table.
#   emu/compare.sh [scenario.txt] [env] [book.epub | host/dir=/card/dir ...]
# Each extra argument is copied onto the card (eink-qemu --put). Without any, the card holds the
# book from make_utf8_epub.py.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
scenario="${1:-$here/reading.txt}"
env="${2:-x4pro}"
shift $(($# < 2 ? $# : 2))
qemu="${EINK_QEMU:-$root/../tools/eink-qemu-0.7.0/qemu/bin/eink-qemu}"
base="${BASE:-$root/../crosspoint-base}/.pio/build/$env"

books=("$@")
if [ ${#books[@]} -eq 0 ]; then
  mkdir -p "$here/books"
  books=("$(python3 "$here/make_utf8_epub.py" "$here/books")")
fi
name="$(basename "$scenario" .txt)-$(basename "${books[0]}" .epub)"
out="$here/runs/$name-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$here/runs"
puts=()
for book in "${books[@]}"; do puts+=(--put "$book"); done

status=0
"$qemu" compare "$base" "$root/.pio/build/$env" "${puts[@]}" --out "$out" --script "$scenario" >"$out.jsonl" || status=$?
sed -n '1,/^<details>/p' "$out/compare.md"
echo "Report: $out/report.html"
exit $status
