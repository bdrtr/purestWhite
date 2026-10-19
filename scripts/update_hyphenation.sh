#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT_DIR"

# The Rust crate embeds these files as they are (rust/crates/bembeyaz-hyphenation/src/languages.rs).
# A download replaces its file only when it is whole: the root address, pointing inside the file.
process() {
  local lang="$1"
  local tmp
  tmp="$(mktemp)"
  wget -O "$tmp" "https://github.com/typst/hypher/raw/refs/heads/main/tries/$lang.bin"
  python3 -c '
import sys
blob = open(sys.argv[1], "rb").read()
if len(blob) < 4 or not 4 <= int.from_bytes(blob[:4], "big") < len(blob):
    sys.exit(sys.argv[1] + ": not a hypher trie")
' "$tmp"
  mv "$tmp" "rust/crates/bembeyaz-hyphenation/tries/$lang.bin"
}

process en
process fr
process de
process es
process ru
process it
process uk
process pl
process pt
process sv
process fi
