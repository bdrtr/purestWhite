#!/usr/bin/env sh
# Downloads the Unicode 17.0.0 bidi conformance tests into unicode/ (not in git), for
#   cargo test --release -p bembeyaz-bidi conformance -- --ignored --nocapture
set -eu
cd "$(dirname "$0")"
mkdir -p unicode
for f in BidiTest.txt BidiCharacterTest.txt DerivedBidiClass.txt BidiBrackets.txt BidiMirroring.txt; do
  case $f in
    DerivedBidiClass.txt) path=extracted/$f ;;
    *) path=$f ;;
  esac
  [ -s "unicode/$f" ] || curl -sfL -o "unicode/$f" "https://www.unicode.org/Public/17.0.0/ucd/$path"
done
