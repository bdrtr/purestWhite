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
positions; account bodies. Then the edges: the longest string and key ArduinoJson keeps and one
byte more (also unterminated, and set from C++), conversions at the u16, u32 and i64 edges, -1e-400, extra-metadata keys holding a NUL.
"""
import random
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "rust/crates/bembeyaz-jsondoc/testdata/kosync-corpus.txt"
JSON = ROOT / ".pio/libdeps/x4pro/ArduinoJson/src"
SEED = 20261103
# The commit whose KOReaderSyncMessages.cpp is ArduinoJson's (before the Rust port).
ORACLE = "524380d5"

INTS = [0, 1, -1, 7, 255, 256, 65535, 65536, -32768, 2147483647, 2147483648, -2147483648,
        -2147483649, 4294967295, 4294967296, 9223372036854775807, 9223372036854775808,
        -9223372036854775808, -9223372036854775809, 18446744073709551615, 18446744073709551616,
        123456789012345678901234567890]
FLOATS = ["0.5", "1.5", "-0.0", "0.1", "3.14159", "1e3", "1E-3", "2.5e+10", "1e38", "3.4e38",
          "3.5e38", "1e39", "1e308", "1.8e308", "1e309", "1e-38", "1e-45", "1e-46", "1e-320",
          "123456789.123", "0.000001", "0.0000001", "12345678", "1234567.5", "9999999.99",
          "1e7", "1e-5", "0.30000000000000004", "00012", "1e400", "1e-400", "7e22", "4.94e-324",
          "0.1234567890123456789", "1" * 40, "1" * 70, "0.99999994", "16777217", "2.5", "100.25",
          "1.17549435e-38", "6.02214076e23", "-273.15", "0.001", "999999.9999", "1e15", "1e16",
          # A token of 63 and 64 bytes (ArduinoJson reads 63), a mantissa just past float's
          # (2^23) and below 2^24, fraction digits past double's mantissa, a negative underflow.
          "1" * 63, "1" * 64, "0.8388609", "0.9999999", "16777.215", "8388.6095",
          "1234567890123456.7", "4503599627370495.5", "0.12345678901234567", "-1e-400", "-1e-46"]
# What ArduinoJson reads its own way, or refuses.
ODD_NUMBERS = [".5", "5.", "+1", "+1.5", "-.5", "1e", "1e+", "1.2.3", "--1", "-", "+", "1-2"]
WORDS = ["", "a", "progress", "x/y", "çiçek", "日本", "😀", "tab\there", "nl\nx", "q\"x", "b\\s",
         "nul\0x", "\x01\x1f", "/a/b[2]/text().3", "é" * 30]
KEYS = ["progress", "percentage", "device", "device_id", "timestamp", "position", "pctQ",
        "spine", "page", "pages", "para", "xpath", "filename", "title", "isbn", "asin", "series",
        "series_index", "k", "a b", "_x", "9", "Z_z"]


def h(data):
    if isinstance(data, str):
        data = data.encode()
    return data.hex() or "-"


def bits(value):
    return struct.pack(">f", value).hex()


def esc(rng, s):
    """A JSON string literal of `s`, with some escapes spelled differently."""
    out = []
    for c in s:
        o = ord(c)
        if c == '"':
            out.append('\\"')
        elif c == "\\":
            out.append("\\\\")
        elif o < 0x20 or (o > 0x7E and rng.random() < 0.3):
            if o > 0xFFFF:
                v = o - 0x10000
                out.append("\\u%04x\\u%04x" % (0xD800 + (v >> 10), 0xDC00 + (v & 0x3FF)))
            else:
                out.append(rng.choice(["\\u%04x", "\\u%04X"]) % o)
        elif c == "/" and rng.random() < 0.5:
            out.append("\\/")
        else:
            out.append(c)
    return '"' + "".join(out) + '"'


def odd_string(rng):
    """A string literal ArduinoJson reads its own way."""
    return rng.choice(["'single'", "'it\\'s'", '"lone \\ud800 x"', '"lo \\udc00"', '"\\ud83d\\ude00"',
                       '"\\u0000z"', '"bad \\x"', '"\\uzzzz"', '"\\u12"', '"raw \x01 ctl"',
                       '"\\b\\f\\n\\r\\t"', '"\\\'"'])


def number(rng):
    roll = rng.random()
    if roll < 0.45:
        return str(rng.choice(INTS))
    if roll < 0.92:
        return rng.choice(FLOATS)
    return rng.choice(ODD_NUMBERS)


def value(rng, depth):
    roll = rng.random()
    if depth > 0 and roll < 0.15:
        return obj(rng, depth - 1)
    if depth > 0 and roll < 0.25:
        items = [value(rng, depth - 1) for _ in range(rng.randrange(4))]
        return "[" + ",".join(items) + "]"
    if roll < 0.45:
        return number(rng)
    if roll < 0.65:
        return esc(rng, rng.choice(WORDS))
    if roll < 0.68:
        return odd_string(rng)
    if roll < 0.69:
        return rng.choice(["tru", "nul", "True", "fals"])
    return rng.choice(["true", "false", "null"])


def key(rng):
    k = rng.choice(KEYS)
    if rng.random() < 0.1:
        return k if k.replace("_", "").isalnum() else '"' + k + '"'
    if rng.random() < 0.05:
        return "'" + k + "'"
    return esc(rng, k)


def obj(rng, depth, keys=None):
    n = rng.randrange(5)
    names = [key(rng) for _ in range(n)] if keys is None else keys
    space = rng.choice(["", " ", "\n\t"])
    return "{" + space + ",".join(f"{k}{space}:{space}{value(rng, depth)}" for k in names) + space + "}"


def deep(rng):
    n = rng.choice([9, 10, 11, 12])
    opening = "".join(rng.choice(["[", '{"a":']) for _ in range(n))
    closing = "".join("]" if c == "[" else "}" for c in reversed(opening.replace('{"a":', "{")))
    return opening + "1" + closing


def broken(rng, text):
    roll = rng.random()
    if roll < 0.3 and text:
        return text[: rng.randrange(len(text))]
    if roll < 0.5 and text:
        at = rng.randrange(len(text))
        return text[:at] + rng.choice(['"', "}", ",", ":", "x", "\0", " ", "'", "\\", "1"]) + text[at + 1:]
    if roll < 0.7:
        return text + rng.choice(["x", " x", ",", "}", "\0junk", "1", ".5", "e5"])
    return rng.choice(["", " ", "\t\n", "x", "{", "[", '"', "{}", "[]", '{"a"}', '{"a":}', "{,}"])


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


def check(driver):
    """Whether `driver` answers every step as the corpus says."""
    run = subprocess.run([driver], input=steps(4000).encode(), capture_output=True, check=True)
    same = run.stdout == OUT.read_bytes()
    print("same as the corpus" if same else "DIFFERENT from the corpus")
    sys.exit(0 if same else 1)


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        check(sys.argv[2])
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 4000
    with tempfile.TemporaryDirectory() as tmp:
        driver = Path(tmp) / "driver"
        messages = Path(tmp) / "KOReaderSyncMessages.cpp"
        source = subprocess.run(["git", "-C", str(ROOT), "show",
                                 f"{ORACLE}:lib/KOReaderSync/KOReaderSyncMessages.cpp"],
                                capture_output=True, check=True)
        messages.write_bytes(source.stdout)
        subprocess.run(["c++", "-std=c++17", "-O1", "-Wall", "-Wextra", f"-isystem{JSON}",
                        f"-I{ROOT}/lib/KOReaderSync", str(Path(__file__).with_name("driver.cpp")),
                        str(messages), "-o", str(driver)], check=True)
        run = subprocess.run([str(driver)], input=steps(count).encode(), capture_output=True,
                             check=True)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(run.stdout)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
