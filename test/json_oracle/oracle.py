"""What the JSON oracles (test/*/make_corpus.py: kosync_messages, json_conversions,
crosspoint_state) share: the seeded JSON value generator, the build of a driver over C++ taken
from git, and the --check of another build against a corpus.

kosync_messages and crosspoint_state build their driver over the C++ that used ArduinoJson (taken
from git, as the tree now holds the Rust shim), write the transcript as a Rust crate's golden
corpus, and with --check run the same steps through a driver built over the shim (see
check_shim.sh). json_conversions builds its driver over ArduinoJson alone.
"""
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# ArduinoJson's headers, the version the firmware pins (after `pio run -e x4pro`).
JSON = ROOT / ".pio/libdeps/x4pro/ArduinoJson/src"

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


def from_git(tmp, commit, names):
    """The files `names` (paths in the repository) as they were at `commit`, written into `tmp`
    under their base names; their paths there."""
    out = []
    for name in names:
        source = subprocess.run(["git", "-C", str(ROOT), "show", f"{commit}:{name}"],
                                capture_output=True, check=True)
        path = Path(tmp) / Path(name).name
        path.write_bytes(source.stdout)
        out.append(path)
    return out


def build(driver, sources, flags):
    """`driver` built from `sources` with ArduinoJson, the extra compiler `flags`, then this
    directory (driver_util.h) and its stubs/ (Arduino.h, Logging.h) on the include path."""
    here = Path(__file__).parent
    subprocess.run(["c++", "-std=c++17", "-O1", "-Wall", "-Wextra", f"-isystem{JSON}", *flags,
                    f"-I{here}", f"-I{here / 'stubs'}", *map(str, sources), "-o", str(driver)],
                   check=True)
    return driver


def run(driver, steps):
    """The transcript `driver` writes for `steps`."""
    return subprocess.run([str(driver)], input=steps.encode(), capture_output=True,
                          check=True).stdout


def check(driver, steps, corpus):
    """Exits with whether `driver` answers `steps` exactly as `corpus` holds."""
    same = run(driver, steps) == corpus.read_bytes()
    print("same as the corpus" if same else "DIFFERENT from the corpus")
    sys.exit(0 if same else 1)
