#!/usr/bin/env python3
"""Builds driver.cpp over the ArduinoJson version of lib/KOReaderSync/KOReaderSyncMessages.cpp
(taken from git, commit 524380d5: the file in the tree is now the Rust shim) with ArduinoJson
7.4.2, runs it on seeded random scenarios and edge steps, and writes the transcript as the golden
corpus the Rust tests replay: rust/crates/bembeyaz-jsondoc/testdata/kosync-corpus.txt.

  test/kosync_messages/make_corpus.py [STEPS]    (default 4000 random steps, and the edge steps;
                                                  the Rust tests want over 1,500 json steps and
                                                  over 1,800 message steps)
  test/kosync_messages/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 4000 and the edge steps) through a driver built
otherwise (driver.cpp over the Rust shim: see check_shim.sh) and says whether its transcript is
the corpus, byte for byte.

Needs a host C++17 compiler (`c++`) and ArduinoJson's headers (the version the firmware pins, in
.pio/libdeps after a PlatformIO build), and git. After regenerating, run `cargo test -p
bembeyaz-jsondoc -p bembeyaz-kosync` in rust/ and check_shim.sh. The corpus only ever holds the
C++ behavior: a deliberate change in the Rust port goes in a commit of its own, with the cases it
changes edited by hand.

Scenarios: JSON of every kind (scalars at the root, nesting past ArduinoJson's limit of 10, single
quotes, unquoted keys, duplicate keys, escapes and surrogates, numbers at every integer and float
edge), much of it then broken (cut, a byte changed, garbage after it); progress answers with each
field of any type; progress updates with any strings and float bits, extra metadata JSON and rich
positions; account bodies. Then the edges: the longest string and key ArduinoJson keeps and one byte
more (also unterminated, and set from C++), conversions at the u16, u32 and i64 edges, -1e-400,
extra-metadata keys holding a NUL.
"""
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
from oracle import (  # noqa: E402
    h, bits, number, value, key, obj, deep, broken, WORDS, ROOT, build, check, from_git, run)

OUT = ROOT / "rust/crates/bembeyaz-jsondoc/testdata/kosync-corpus.txt"
SEED = 20261103
# The commit whose KOReaderSyncMessages.cpp is ArduinoJson's (before the Rust port).
ORACLE = "524380d5"

def json_line(rng):
    roll = rng.random()
    if roll < 0.15:
        text = number(rng) + rng.choice(["", "", " ", "x", ",", "\0", " 1"])
    elif roll < 0.2:
        text = deep(rng)
    else:
        text = value(rng, 3) if rng.random() < 0.3 else obj(rng, 3)
    if rng.random() < 0.2:
        text = broken(rng, text)
    return f"json {h(text)}"


def progress_answer(rng):
    fields = rng.sample(["progress", "percentage", "device", "device_id", "timestamp", "position"],
                        rng.randrange(7))
    parts = []
    for f in fields:
        if f == "position" and rng.random() < 0.7:
            pos = rng.sample(["pctQ", "spine", "page", "pages", "para", "xpath"], rng.randrange(7))
            v = "{" + ",".join(f'"{p}":{value(rng, 1)}' for p in pos) + "}"
        else:
            v = value(rng, 1)
        parts.append(f'"{f}":{v}')
    text = "{" + ",".join(parts) + "}"
    if rng.random() < 0.1:
        text = value(rng, 2)
    if rng.random() < 0.15:
        text = broken(rng, text)
    return f"parse {rng.choice([0, 1])} {h(rng.choice(WORDS[:5]))} {h(text)}"


def float_bits(rng):
    roll = rng.random()
    if roll < 0.4:
        return bits(rng.choice([0.0, -0.0, 0.5, 0.1234, 1.0, 0.3333333, 1e-7, 1e-5, 1e7, 12345678.0,
                                0.99999994, 3.4e38, 1e-40, 0.123456789]))
    if roll < 0.5:
        return rng.choice(["7fc00000", "7f800000", "ff800000", "00000001", "80000000", "7f7fffff"])
    return "%08x" % rng.getrandbits(32)


def word(rng):
    return h(rng.choice(WORDS)) if rng.random() < 0.8 else h("x" * rng.choice([119, 120, 121]))


def extra_json(rng):
    roll = rng.random()
    if roll < 0.3:
        return "-"
    if roll < 0.8:
        return h(obj(rng, 1))
    return h(rng.choice([value(rng, 1), broken(rng, obj(rng, 1))]))


def progress_update(rng):
    meta = "-"
    if rng.random() < 0.7:
        series = float_bits(rng) if rng.random() < 0.5 else "-"
        meta = ",".join([word(rng), word(rng), word(rng), extra_json(rng), word(rng), word(rng),
                         word(rng), series])
    pos = "-"
    if rng.random() < 0.7:
        para = str(rng.choice([0, 1, 65535])) if rng.random() < 0.5 else "-"
        pos = ",".join([str(rng.choice([0, 1, 4294967295, rng.getrandbits(32)])),
                        str(rng.choice([0, 65535, rng.getrandbits(16)])),
                        str(rng.getrandbits(16)), str(rng.getrandbits(16)), para, word(rng)])
    return (f"body {rng.choice([0, 1])} {rng.choice([0, 1])} {word(rng)} {word(rng)} "
            f"{float_bits(rng)} {meta} {pos}")


def line(rng):
    roll = rng.random()
    if roll < 0.45:
        return json_line(rng)
    if roll < 0.75:
        return progress_answer(rng)
    if roll < 0.95:
        return progress_update(rng)
    return f"user {word(rng)} {word(rng)}"


def edges():
    """Steps at the conversions' edges, too narrow for the random ones to hit."""
    answers = [
        '{"percentage":-1e-400,"timestamp":"-1e-400"}',
        '{"percentage":"-1e-400"}',
        '{"position":{"spine":65536.0,"page":65535.5,"pages":-0.5,"para":1e10,'
        '"pctQ":4294967296.0}}',
        '{"position":{"spine":65535.0,"page":-1,"pages":65535,"para":65536,"pctQ":4294967040.0}}',
        '{"position":{"pctQ":"4294967296.0","spine":"65536","page":"-0"},"timestamp":9.3e18}',
        '{"timestamp":9223372036854774784.0,"position":{"pctQ":-0.0}}',
    ]
    out = [f"parse 1 - {h(a)}" for a in answers]
    for extra in ['{"a\\u0000b":1,"title\\u0000x":2,"isbn\\u0000":3}', '{"\\u0000":true}']:
        meta = ",".join(["66", "74", "61", h(extra), "-", "-", "-", "-"])
        out.append(f"body 1 0 64 70 3f000000 {meta} -")
    return out


def steps(count):
    rng = random.Random(SEED)
    out = "\n".join(line(rng) for _ in range(count)) + "\n"
    # ArduinoJson's longest string, 65,535 bytes, and one more: as a value, a key, an unterminated
    # string, and set from C++.
    out += "long 65535\nlong 65536\n"
    return out + "\n".join(edges()) + "\n"


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        check(sys.argv[2], steps(4000), OUT)
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 4000
    with tempfile.TemporaryDirectory() as tmp:
        messages = from_git(tmp, ORACLE, ["lib/KOReaderSync/KOReaderSyncMessages.cpp"])
        driver = build(Path(tmp) / "driver", [Path(__file__).with_name("driver.cpp"), *messages],
                       [f"-I{ROOT}/lib/KOReaderSync"])
        transcript = run(driver, steps(count))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
