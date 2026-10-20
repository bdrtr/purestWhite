#!/usr/bin/env bash
# Builds the release images for the version in platformio.ini and drafts a
# GitHub release of bdrtr/purestWhite with them. The draft is published by
# hand after the hardware tests: devices update from the latest published
# release (Settings > System > Check for updates).
#
#   scripts/release.sh            # build, then create the draft
#   scripts/release.sh --dry-run  # build and list what would be uploaded
set -euo pipefail

REPO="bdrtr/purestWhite"
cd "$(dirname "$0")/.."

dry_run=false
[[ "${1:-}" == "--dry-run" ]] && dry_run=true

version=$(python3 -c 'import configparser; c = configparser.ConfigParser(); c.read("platformio.ini"); print(c["crosspoint"]["version"])')
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "platformio.ini [crosspoint] version must be major.minor.patch, got '$version'" >&2
  exit 1
fi
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  echo "The working tree has uncommitted changes; commit them first." >&2
  exit 1
fi
commit=$(git rev-parse HEAD)
if ! $dry_run; then
  if gh release view "$version" --repo "$REPO" >/dev/null 2>&1; then
    echo "Release $version already exists on $REPO; raise the version in platformio.ini." >&2
    exit 1
  fi
  if ! git branch -r --contains "$commit" | grep -q 'purestwhite/main'; then
    echo "HEAD is not on purestwhite/main yet; push it first." >&2
    exit 1
  fi
fi

pio=$(command -v pio || echo "$HOME/.platformio/penv/bin/pio")
dist="dist/$version"
rm -rf "$dist"
mkdir -p "$dist"
# The asset names are what OtaUpdater looks for: purestWhite-<version>-<board tag>.bin.
for board in x4pro x4c; do
  "$pio" run -e "$board-gh_release"
  cp ".pio/build/$board-gh_release/firmware.bin" "$dist/purestWhite-$version-$board.bin"
done
(cd "$dist" && sha256sum ./*.bin > SHA256SUMS)
cat "$dist/SHA256SUMS"

if $dry_run; then
  echo "Dry run: would draft release $version of $REPO at $commit with the files above."
  exit 0
fi
gh release create "$version" "$dist"/*.bin "$dist/SHA256SUMS" \
  --repo "$REPO" --target "$commit" --draft --generate-notes \
  --title "purestWhite $version"
echo "Drafted release $version. Publish it on GitHub after the hardware tests."
