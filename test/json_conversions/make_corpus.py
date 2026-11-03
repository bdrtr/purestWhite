#!/usr/bin/env python3
"""Builds driver.cpp with ArduinoJson 7.4.2, runs it on seeded member values and writes the
transcript as the golden corpus the Rust tests replay:
rust/crates/bembeyaz-jsondoc/testdata/conversions-corpus.txt.

  test/json_conversions/make_corpus.py [STEPS]    (default 3000 random steps, and the edge steps)

Needs a host C++17 compiler (`c++`) and ArduinoJson's headers (the version the firmware pins, in
.pio/libdeps after a PlatformIO build). After regenerating, run `cargo test -p bembeyaz-jsondoc`
in rust/.

Values: every kind the shared generator (test/json_oracle/oracle.py) makes (integers at every edge,
floats, odd numbers, strings, odd strings, arrays, objects), numbers written as strings, and the
edges of the types the settings stores read (u8, u16, u32, int).
"""
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-jsondoc/testdata/conversions-corpus.txt"
SEED = 20261104
EDGES = ["255", "256", "-1", "65535", "65536", "4294967295", "4294967296", "2147483647",
         "2147483648", "-2147483648", "-2147483649", "9223372036854775807", "9223372036854775808",
         "255.0", "255.5", "256.0", "-0.5", "-1.0", "65535.9", "2147483520.0", "2147483648.0",
         "4294967040.0", "4294967296.0", "1e10", "-1e10", "true", "false", "null", "\"7\"",
         "\"255\"", "\"256\"", "\"-1\"", "\"1.5\"", "\"x\"", "\"\"", "\"a\\u0000b\"", "[]", "[1]",
         "[1,2,3,4,5,6]", "[70000]", "[\"3\"]", "[-1,2]", "{}", "{\"a\":1}", "[[1]]",
         # Doubles (not exact as float) just inside and outside int's and u8's range.
         "2147483647.5", "2147483647.25", "-2147483648.5", "-2147483648.25", "255.0000001",
         "-0.0000001", "65535.0000001", "4294967295.5", "4294967295.25"]


def member(rng):
    roll = rng.random()
    if roll < 0.15:
        return '"' + oracle.number(rng) + '"'
    if roll < 0.3:
        return rng.choice(EDGES)
    return oracle.value(rng, 2)


def steps(count):
    rng = random.Random(SEED)
    docs = ['{"v":' + member(rng) + "}" for _ in range(count)]
    docs += ['{"v":' + e + "}" for e in EDGES] + ["{}", "[1]", "5", '"s"']
    return "".join(f"v {oracle.h(d)}\n" for d in docs)


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 3000
    with tempfile.TemporaryDirectory() as tmp:
        driver = oracle.build(Path(tmp) / "driver", [Path(__file__).with_name("driver.cpp")], [])
        transcript = oracle.run(driver, steps(count))
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(oracle.ROOT)}")


if __name__ == "__main__":
    main()
