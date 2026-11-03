#!/usr/bin/env python3
"""Builds driver.cpp over the C++ versions of src/OpdsServerStore and lib/Serialization's
PersistableStore (taken from git, commit 485d3886: the ones in the tree are now the Rust shim) with
ArduinoJson 7.4.2 and the host stubs (stubs/, and those of test/json_oracle), runs it on seeded
scenarios and writes the transcript as the golden corpus the Rust tests replay:
rust/crates/bembeyaz-store/testdata/opds-servers-corpus.txt.

  test/opds_servers/make_corpus.py [STEPS]    (default 1000 random steps, and the edge steps)
  test/opds_servers/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 1000 and the edge steps) through another build of
driver.cpp (over the Rust shim: see check_shim.sh) and says whether its transcript is the corpus,
byte for byte.

Needs a host C++17 compiler (`c++`), ArduinoJson's headers (.pio/libdeps after a PlatformIO
build) and git. After regenerating, run `cargo test -p bembeyaz-store` in rust/ and
test/opds_servers/check_shim.sh. The corpus only ever holds the C++ behavior.

Scenarios: opds.json files with server lists of any length (past the store's 8), items that are
not objects, fields of any type (the shared value generator, test/json_oracle/oracle.py),
obfuscated passwords that decode, do not decode or are empty, legacy plain passwords (which the
store resaves obfuscated), header lists of any length (past 2) and kind, a missing or wrong-typed
"servers", broken and empty files; servers added after them (past the limit too); edges: roots
that are not objects, and the longest string ArduinoJson keeps and one byte more.
"""
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-store/testdata/opds-servers-corpus.txt"
SEED = 20261107
# The commit whose store files are the C++ store (before the Rust port).
ORACLE = "485d3886"
ORACLE_FILES = ["src/OpdsServerStore.cpp", "src/OpdsServerStore.h",
                "lib/Serialization/PersistableStore.cpp", "lib/Serialization/PersistableStore.h"]
TEXTS = ["", "Kitaplık", "https://opds.example.org/catalog", "http://192.0.2.1:8080/opds",
         "okur", "p@ss wörd", "CF-Access-Client-Id", "a\0b", "x" * 40]


def obf(text):
    """The host fake's obfuscation (stubs/ObfuscationFake.cpp)."""
    return "~" + text.encode().hex() if text else ""


def obfuscated(rng):
    roll = rng.random()
    if roll < 0.5:
        return oracle.esc(rng, obf(rng.choice(TEXTS + oracle.WORDS)))
    if roll < 0.7:
        return oracle.esc(rng, rng.choice(["", "~", "~z", "~abc", "~zz", "plain", "~0"]))
    return oracle.value(rng, 1)


def text_value(rng):
    if rng.random() < 0.15:
        return oracle.value(rng, 1)
    return oracle.esc(rng, rng.choice(TEXTS + oracle.WORDS))


def members(rng, fields):
    names = [n for n in fields if rng.random() < 0.8]
    if rng.random() < 0.1:
        names.append(rng.choice(oracle.KEYS))
    rng.shuffle(names)
    return names


def header(rng):
    if rng.random() < 0.1:
        return oracle.value(rng, 1)
    names = members(rng, ["name", "value_obf"])
    return "{" + ",".join(f'"{n}":' + (obfuscated(rng) if n == "value_obf" else text_value(rng))
                          for n in names) + "}"


def server(rng):
    if rng.random() < 0.06:
        return oracle.value(rng, 1)
    out = []
    for n in members(rng, ["name", "url", "username", "password_obf", "password", "headers"]):
        if n == "password_obf":
            value = obfuscated(rng)
        elif n == "headers":
            count = rng.choice([0, 1, 2, 2, 3])
            value = ("[" + ",".join(header(rng) for _ in range(count)) + "]"
                     if rng.random() < 0.9 else oracle.value(rng, 1))
        else:
            value = text_value(rng)
        out.append(f'"{n}":{value}')
    return "{" + ",".join(out) + "}"


def opds_json(rng):
    if rng.random() < 0.05:
        servers = oracle.value(rng, 1)
    else:
        n = rng.choice([0, 1, 2, 3, 7, 8, 9, 11])
        servers = "[" + ",".join(server(rng) for _ in range(n)) + "]"
    parts = [f'"servers":{servers}'] if rng.random() < 0.95 else []
    if rng.random() < 0.1:
        parts.append(f'"{rng.choice(oracle.KEYS)}":{oracle.value(rng, 1)}')
    text = "{" + ",".join(parts) + "}"
    if rng.random() < 0.05:
        text = oracle.broken(rng, text)
    return text


def line(rng):
    if rng.random() < 0.25:
        return "add " + " ".join(oracle.h(rng.choice(TEXTS + oracle.WORDS)) for _ in range(8))
    return f"load {oracle.h(opds_json(rng))}"


def edges():
    docs = ["{}", "[]", "5", "null", '{"servers":{}}', '{"servers":null}', "",
            '{"servers":[{"password":"legacy"}]}', '{"servers":[{"password_obf":"~zz","password":"x"}]}',
            '{"servers":[{"password_obf":"","password":""}]}']
    out = [f"load {oracle.h(d)}" for d in docs]
    # ArduinoJson's longest string, 65,535 bytes, and one more, read and written.
    return out + ["long 65535", "long 65536"]


def steps(count):
    rng = random.Random(SEED)
    return "\n".join([line(rng) for _ in range(count)] + edges()) + "\n"


def flags(store_dir):
    here = Path(__file__).parent
    return ["-DARDUINOJSON_ENABLE_ARDUINO_STRING=1", f"-I{store_dir}", f"-I{here / 'stubs'}", f"-I{oracle.ROOT / 'lib/Serialization'}",
            f"-I{oracle.ROOT / 'lib/HttpHeader'}", f"-I{oracle.ROOT / 'src'}"]


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        oracle.check(sys.argv[2], steps(1000), OUT)
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    here = Path(__file__).parent
    with tempfile.TemporaryDirectory() as tmp:
        files = oracle.from_git(tmp, ORACLE, ORACLE_FILES)
        sources = [here / "driver.cpp", here / "stubs/ObfuscationFake.cpp", files[0], files[2]]
        driver = oracle.build(Path(tmp) / "driver", sources, flags(tmp))
        transcript = oracle.run(driver, steps(count))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(oracle.ROOT)}")


if __name__ == "__main__":
    main()
