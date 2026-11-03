#!/usr/bin/env python3
"""Builds driver.cpp over the C++ version of src/RecentBooksStore (taken from git, commit 6a089cc7:
the store in the tree is now the Rust shim) with ArduinoJson 7.4.2 and the host stubs of
test/json_oracle, runs it on seeded scenarios and writes the transcript as the golden corpus the
Rust tests replay: rust/crates/bembeyaz-store/testdata/recent-books-corpus.txt.

  test/recent_books/make_corpus.py [STEPS]    (default 1000 random steps, and the edge steps)
  test/recent_books/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 1000 and the edge steps) through another build of
driver.cpp (over the Rust shim: see check_shim.sh) and says whether its transcript is the corpus,
byte for byte.

Needs a host C++17 compiler (`c++`), ArduinoJson's headers (.pio/libdeps after a PlatformIO
build) and git. After regenerating, run `cargo test -p bembeyaz-store` in rust/ and
test/recent_books/check_shim.sh. The corpus only ever holds the C++ behavior.

Scenarios: recent.json files with lists of any length (past the store's 10), items that are not
objects, fields of any type (the shared value generator, test/json_oracle/oracle.py), a missing
or wrong-typed "books", broken text; books added after them (any bytes in their fields, a NUL
too); edges: roots that are not objects or have no "books" array, read and written; and, each
after {} empties the store, the longest string ArduinoJson keeps and one byte more.
"""
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-store/testdata/recent-books-corpus.txt"
SEED = 20261106
# The commit whose store files are the C++ store (before the Rust port).
ORACLE = "6a089cc7"
ORACLE_FILES = ["src/RecentBooksStore.cpp", "src/RecentBooksStore.h",
                "lib/Serialization/PersistableStore.h"]
FIELDS = ["path", "title", "author", "coverBmpPath"]
TEXTS = ["", "/books/a.epub", "/Kitaplar/Çiçekli Şiir.epub", "Çiçekli Şiir", "日本の本",
         "Ahmet Ümit", "/.crosspoint/epub_123/cover.bmp", "a\0b", "x" * 40]


def field_value(rng):
    if rng.random() < 0.2:
        return oracle.value(rng, 1)
    return oracle.esc(rng, rng.choice(TEXTS + oracle.WORDS))


def book(rng):
    if rng.random() < 0.08:
        return oracle.value(rng, 1)
    names = [f for f in FIELDS if rng.random() < 0.85]
    if rng.random() < 0.1:
        names.append(rng.choice(oracle.KEYS))
    rng.shuffle(names)
    return "{" + ",".join(f'"{n}":{field_value(rng)}' for n in names) + "}"


def recent_json(rng):
    roll = rng.random()
    if roll < 0.05:
        books = oracle.value(rng, 1)
    else:
        n = rng.choice([0, 1, 2, 3, 5, 9, 10, 11, 14])
        books = "[" + ",".join(book(rng) for _ in range(n)) + "]"
    members = [f'"books":{books}'] if rng.random() < 0.95 else []
    if rng.random() < 0.1:
        members.append(f'"{rng.choice(oracle.KEYS)}":{oracle.value(rng, 1)}')
    rng.shuffle(members)
    text = "{" + ",".join(members) + "}"
    if rng.random() < 0.05:
        text = oracle.broken(rng, text)
    return text


def line(rng):
    if rng.random() < 0.3:
        fields = [oracle.h(rng.choice(TEXTS + oracle.WORDS)) for _ in range(4)]
        if rng.random() < 0.3:
            fields[0] = oracle.h(rng.choice(["/books/a.epub", "/Kitaplar/Çiçekli Şiir.epub"]))
        return "add " + " ".join(fields)
    return f"from {oracle.h(recent_json(rng))}"


def edges():
    out = [f"from {oracle.h(d)}" for d in ['{}', '[]', '5', 'null', '{"books":{}}', '{"books":null}']]
    # ArduinoJson's longest string, 65,535 bytes, and one more, read and written.
    return out + ["from 7b7d", "long 65535", "from 7b7d", "long 65536"]


def steps(count):
    rng = random.Random(SEED)
    return "\n".join([line(rng) for _ in range(count)] + edges()) + "\n"


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        oracle.check(sys.argv[2], steps(1000), OUT)
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    here = Path(__file__).parent
    with tempfile.TemporaryDirectory() as tmp:
        store = oracle.from_git(tmp, ORACLE, ORACLE_FILES)
        driver = oracle.build(Path(tmp) / "driver", [here / "driver.cpp", store[0]],
                              [f"-I{tmp}"])
        transcript = oracle.run(driver, steps(count))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(oracle.ROOT)}")


if __name__ == "__main__":
    main()
