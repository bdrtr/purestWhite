"""Writes two SD card folders whose settings make the reader use an SD card font (SdCardFont,
bembeyaz-font):

- good/: the font as given
- bad/: the same font with the first style's second interval overlapping its first, which load
  refuses

Each holds .fonts/<family>/<family>_<size>.cpfont and a .crosspoint/settings.json that selects the
family. Make the font with lib/EpdFont/scripts/fontconvert_sdcard.py (it needs freetype-py), e.g.
--intervals latin-ext --size 14 --regular LiberationSerif-Regular.ttf --bold ... -o Font.cpfont.

  python3 emu/make_sd_font_cards.py FONT.cpfont FAMILY SIZE OUT_DIR
  emu/compare.sh emu/reading.txt x4pro BOOK.epub OUT_DIR/good/.fonts=/ OUT_DIR/good/.crosspoint=/
"""
import json
import struct
import sys
from pathlib import Path

font, family, size, out_dir = Path(sys.argv[1]), sys.argv[2], int(sys.argv[3]), Path(sys.argv[4])
data = font.read_bytes()
if data[:8] != b"CPFONT\0\0" or struct.unpack_from("<I", data, 32 + 4)[0] < 2:
    sys.exit("expected a .cpfont whose first style has two intervals or more")

bad = bytearray(data)
intervals = struct.unpack_from("<I", bad, 32 + 24)[0]
first_last = struct.unpack_from("<I", bad, intervals + 4)[0]
struct.pack_into("<I", bad, intervals + 12, first_last)

for name, content in (("good", data), ("bad", bytes(bad))):
    card = out_dir / name
    folder = card / ".fonts" / family
    folder.mkdir(parents=True, exist_ok=True)
    (folder / f"{family}_{size}.cpfont").write_bytes(content)
    (card / ".crosspoint").mkdir(exist_ok=True)
    (card / ".crosspoint" / "settings.json").write_text(json.dumps({"sdFontFamilyName": family}))
    print(card)
