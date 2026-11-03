#!/usr/bin/env python3
"""Builds driver.cpp over the C++ versions of src/util/BookmarkFile and lib/Serialization's
PersistableStore (taken from git, commit 1bcf42c3: the one in the tree is now the Rust shim)
with ArduinoJson 7.4.2 and the host stubs of test/json_oracle, runs it on seeded scenarios and
writes the transcript as the golden corpus the Rust tests replay:
rust/crates/bembeyaz-store/testdata/bookmark-file-corpus.txt.

  test/bookmark_file/make_corpus.py [STEPS]    (default 1000 random steps, and the edge steps)
  test/bookmark_file/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 1000 and the edge steps) through another build of
driver.cpp (over the Rust shim: see check_shim.sh) and says whether its transcript is the corpus,
byte for byte.

Needs a host C++17 compiler (`c++`), ArduinoJson's headers (.pio/libdeps after a PlatformIO
build) and git. After regenerating, run `cargo test -p bembeyaz-store` in rust/ and
test/bookmark_file/check_shim.sh. The corpus only ever holds the C++ behavior.

Scenarios: bookmark files with lists of any length, items that are not objects, fields of any type
(the shared value generator, test/json_oracle/oracle.py), names up to and past the 128 bytes kept,
percentages of every float kind, page numbers at and past u16's edges, visible-text offsets present,
null, missing or of another type, a missing or wrong-typed "bookmarks", broken and empty files;
bookmarks saved (names past 128 refused, NaN and infinite percentages, with and without an offset);
edges: an empty object, roots that are not objects, an empty file, a "bookmarks" that is not an
array, an offset that is null or a string, and the longest string ArduinoJson keeps and one byte
more.
"""
import random
import struct
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-store/testdata/bookmark-file-corpus.txt"
SEED = 20261111
# The commit whose BookmarkFile is the C++ one (before the Rust port).
ORACLE = "1bcf42c3"
ORACLE_FILES = ["src/util/BookmarkFile.cpp", "lib/Serialization/PersistableStore.cpp",
                "lib/Serialization/PersistableStore.h", "lib/Serialization/StoredPassword.cpp",
                "lib/Serialization/StoredPassword.h"]
XPATHS = ["", "/body/DocFragment[3]/body/p[2]/text().14", "x", "a\0b"]
SUMMARIES = ["", "Çiçekli bir sabahtı", "日本語の本", "Line\nbreak"]
NAMES = ["", "Önemli", "n" * 128, "n" * 129, "é" * 64, "é" * 65, "a\0b"]
NUMBERS = ["0", "1", "65535", "65536", "-1", "1.5", "2.0", "1e3", '"7"', "true", "null", "[]"]
PERCENTAGES = ["0", "0.5", "1", "0.333333", "1.5", "-0.25", "1e-7", "3.4e38", "1e39", "2", '"0.5"',
               "true", "null", "{}"]
FLOATS = [0.0, 0.5, 0.25, 1.0, 0.1, 0.333333343, 1e-7, float("nan"), float("inf"), -0.0, 12.75]


def value(rng, choices):
    return rng.choice(choices) if rng.random() < 0.85 else oracle.value(rng, 1)


def text(rng, choices):
    return oracle.esc(rng, rng.choice(choices + oracle.WORDS)) if rng.random() < 0.9 \
        else oracle.value(rng, 1)


def bookmark(rng):
    if rng.random() < 0.05:
        return oracle.value(rng, 1)
    fields = {"xpath": text(rng, XPATHS), "percentage": value(rng, PERCENTAGES),
              "summary": text(rng, SUMMARIES), "name": text(rng, NAMES),
              "si": value(rng, NUMBERS), "pc": value(rng, NUMBERS), "pp": value(rng, NUMBERS),
              "vo": value(rng, NUMBERS + ["4294967295", "4294967296", "123456"])}
    names = [n for n in fields if rng.random() < 0.8]
    if rng.random() < 0.05:
        names.append(rng.choice(oracle.KEYS))
        fields[names[-1]] = oracle.value(rng, 1)
    rng.shuffle(names)
    return "{" + ",".join(f'"{n}":{fields[n]}' for n in names) + "}"


def bookmark_file(rng):
    if rng.random() < 0.05:
        items = oracle.value(rng, 1)
    else:
        items = "[" + ",".join(bookmark(rng) for _ in range(rng.choice([0, 1, 2, 3, 5, 10]))) + "]"
    doc = "{" + (f'"bookmarks":{items}' if rng.random() < 0.95 else "") + "}"
    return oracle.broken(rng, doc) if rng.random() < 0.05 else doc


def entry(rng):
    h = oracle.h
    bits = struct.pack(">f", rng.choice(FLOATS)).hex()
    numbers = [str(rng.choice([0, 1, 7, 65535, rng.randrange(65536)])) for _ in range(3)]
    vo = rng.choice(["-", "0", "123", "4294967295"])
    texts = [h(rng.choice(XPATHS)), h(rng.choice(SUMMARIES)), h(rng.choice(NAMES))]
    return ",".join(texts + [bits] + numbers + [vo])


def line(rng):
    if rng.random() < 0.25:
        return "save " + " ".join(entry(rng) for _ in range(rng.choice([0, 1, 2, 4])))
    return f"load {oracle.h(bookmark_file(rng))}"


def edges():
    docs = ["{}", "[]", "5", "null", "", '{"bookmarks":{}}', '{"bookmarks":[{"vo":null}]}',
            '{"bookmarks":[{"vo":"x"}]}']
    out = [f"load {oracle.h(d)}" for d in docs]
    # ArduinoJson's longest string, 65,535 bytes, and one more, read and written.
    return out + ["long 65535", "long 65536"]


def steps(count):
    rng = random.Random(SEED)
    return "\n".join([line(rng) for _ in range(count)] + edges()) + "\n"


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        oracle.check(sys.argv[2], steps(1000), OUT)
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    here = Path(__file__).parent
    with tempfile.TemporaryDirectory() as tmp:
        files = oracle.from_git(tmp, ORACLE, ORACLE_FILES)
        (Path(tmp) / "util").mkdir()
        sources = [here / "driver.cpp", files[0], files[1], files[3],
                   oracle.ROOT / "src/util/BookmarkUtil.cpp", oracle.STUBS / "ObfuscationFake.cpp"]
        flags = ["-DARDUINOJSON_ENABLE_ARDUINO_STRING=1", f"-I{tmp}",
                 f"-I{oracle.ROOT / 'lib/Serialization'}", f"-I{oracle.ROOT / 'src'}",
                 f"-I{oracle.ROOT / 'src/util'}"]
        driver = oracle.build(Path(tmp) / "driver", sources, flags)
        transcript = oracle.run(driver, steps(count))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(oracle.ROOT)}")


if __name__ == "__main__":
    main()
