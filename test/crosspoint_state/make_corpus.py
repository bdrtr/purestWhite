#!/usr/bin/env python3
"""Builds driver.cpp over the C++ version of src/CrossPointState (taken from git, commit 9038566b:
the store in the tree is now the Rust shim) with ArduinoJson 7.4.2 and the host stubs in stubs/,
runs it on seeded scenarios and writes the transcript as the golden corpus the Rust tests replay:
rust/crates/bembeyaz-store/testdata/state-corpus.txt.

  test/crosspoint_state/make_corpus.py [STEPS]    (default 1500 random steps, and the edge steps)
  test/crosspoint_state/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 1500 and the edge steps) through another build of
driver.cpp (over the Rust shim: see check_shim.sh) and says whether its transcript is the corpus,
byte for byte.

Needs a host C++17 compiler (`c++`), ArduinoJson's headers (.pio/libdeps after a PlatformIO
build) and git. After regenerating, run `cargo test -p bembeyaz-store` in rust/ and
test/crosspoint_state/check_shim.sh. The corpus only ever holds the C++ behavior.

Scenarios: state files with each field of any type (the shared value generator,
test/json_oracle/oracle.py), image lists of any length and item, positions and fills past the ring,
the legacy lastSleepImage, missing fields; the ring asked and pushed after each; states written
directly, with any path and numbers.
"""
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-store/testdata/state-corpus.txt"
SEED = 20261105
# The commit whose store files are the C++ store (before the Rust port).
ORACLE = "9038566b"
ORACLE_FILES = ["src/CrossPointState.h", "src/CrossPointState.cpp",
                "lib/Serialization/PersistableStore.h"]
FIELDS = ["openEpubPath", "recentSleepImages", "recentSleepPos", "recentSleepFill",
          "recentOverlaySleepImages", "recentOverlaySleepPos", "recentOverlaySleepFill",
          "readerActivityLoadCount", "lastSleepFromReader", "showBootScreen", "lastSleepImage"]
SMALL = ["0", "1", "3", "15", "16", "17", "31", "255", "256", "-1", "1.5", "true", "\"4\"", "null"]
IMAGES = ["0", "1", "7", "65535", "65536", "-1", "2.5", "\"9\"", "true", "null", "[]", "{}"]
PATHS = ["", "/books/a.epub", "/Kitaplar/Çiçekli Şiir.epub", "/日本/本.epub", "a\0b", "x" * 300]


def images(rng):
    if rng.random() < 0.1:
        return oracle.value(rng, 1)
    n = rng.choice([0, 1, 2, 5, 15, 16, 17, 20])
    items = [rng.choice(IMAGES) if rng.random() < 0.5 else str(rng.randrange(70000))
             for _ in range(n)]
    return "[" + ",".join(items) + "]"


def field_value(rng, name):
    roll = rng.random()
    if roll < 0.15:
        return oracle.value(rng, 1)
    if name == "openEpubPath":
        return oracle.esc(rng, rng.choice(PATHS + oracle.WORDS))
    if name.endswith("Images"):
        return images(rng)
    if name in ("lastSleepFromReader", "showBootScreen"):
        return rng.choice(["true", "false", "0", "1", "null", "\"true\""])
    return rng.choice(SMALL) if roll < 0.6 else str(rng.randrange(300))


def state_json(rng):
    names = [f for f in FIELDS if rng.random() < 0.7]
    rng.shuffle(names)
    members = [f'"{n}":{field_value(rng, n)}' for n in names]
    if rng.random() < 0.1:
        members.append(f'"{rng.choice(oracle.KEYS)}":{oracle.value(rng, 1)}')
    text = "{" + ",".join(members) + "}"
    if rng.random() < 0.05:
        text = oracle.broken(rng, text)
    return text


def number_list(rng):
    return ",".join(str(rng.choice([0, 1, 9, 65535, rng.randrange(65536)])) for _ in range(16))


def to_line(rng):
    path = rng.choice(PATHS + oracle.WORDS)
    nums = [str(rng.choice([0, 1, 15, 16, 200, 255, rng.randrange(256)])) for _ in range(5)]
    flags = [str(rng.randrange(2)) for _ in range(2)]
    return " ".join(["to", oracle.h(path), number_list(rng), nums[0], nums[1], number_list(rng),
                     nums[2], nums[3], nums[4]] + flags)


def line(rng):
    if rng.random() < 0.2:
        return to_line(rng)
    idx = rng.choice([0, 1, 7, 65535, rng.randrange(70000) % 65536])
    count = rng.choice([0, 1, 3, 16, 17, 255])
    push = rng.choice([0, 7, 65535, rng.randrange(65536)])
    return f"from {oracle.h(state_json(rng))} {idx} {count} {push}"


def edges():
    docs = ['{}', '{"lastSleepImage":7}', '{"lastSleepImage":255}', '{"lastSleepImage":256}',
            '{"lastSleepImage":7,"recentSleepFill":1,"recentSleepImages":[1]}',
            '{"lastSleepImage":7,"recentSleepFill":1}',
            '{"recentSleepPos":20,"recentSleepImages":[]}',
            '{"recentSleepPos":20,"recentSleepImages":[1]}',
            '{"recentSleepFill":9,"recentSleepImages":[1,2]}',
            '[]', '5', '"x"', 'null']
    return [f"from {oracle.h(d)} 7 16 7" for d in docs]


def steps(count):
    rng = random.Random(SEED)
    return "\n".join([line(rng) for _ in range(count)] + edges()) + "\n"


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        oracle.check(sys.argv[2], steps(1500), OUT)
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1500
    here = Path(__file__).parent
    with tempfile.TemporaryDirectory() as tmp:
        store = oracle.from_git(tmp, ORACLE, ORACLE_FILES)
        driver = oracle.build(Path(tmp) / "driver", [here / "driver.cpp", store[1]],
                              [f"-I{tmp}", f"-I{here / 'stubs'}"])
        transcript = oracle.run(driver, steps(count))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(oracle.ROOT)}")


if __name__ == "__main__":
    main()
