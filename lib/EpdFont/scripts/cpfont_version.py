# Canonical version constants for the .cpfont binary format and font manifest.
#
# These are the single source of truth for the build tooling. The CI workflow
# (release-fonts.yml) and both Python scripts (fontconvert_sdcard.py,
# generate-font-manifest.py) read from here.
#
# The firmware carries its own copies of CPFONT_VERSION: SdCardFont.h,
# BB_CPFONT_FORMAT_VERSION in rust/include/bembeyaz/cpfont.h and VERSION in
# rust/crates/bembeyaz-font/src/cpfont.rs (with the assert in
# rust/crates/bembeyaz-ffi/src/cpfont.rs). Bump them by hand when the firmware
# is updated to support a new version.

# .cpfont binary format version. Bump when the on-disk struct layout changes.
CPFONT_VERSION = 4

# JSON manifest schema version. Bump when the manifest shape changes.
FONTS_MANIFEST_VERSION = 1
