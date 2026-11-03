#!/usr/bin/env python3
"""Builds driver.cpp over the C++ versions of lib/KOReaderSync/KOReaderCredentialStore,
lib/Serialization's PersistableStore and StoredPassword (taken from git, commit 4f1317f3: the store
in the tree is now the Rust shim) with ArduinoJson 7.4.2 and the host stubs of test/json_oracle,
runs it on seeded scenarios and writes the transcript as the golden corpus the Rust tests replay:
rust/crates/bembeyaz-store/testdata/koreader-credentials-corpus.txt.

  test/koreader_credentials/make_corpus.py [STEPS]   (default 1000 random steps, and the edge steps)
  test/koreader_credentials/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 1000 and the edge steps) through another build of
driver.cpp (over the Rust shim: see check_shim.sh) and says whether its transcript is the corpus,
byte for byte.

Needs a host C++17 compiler (`c++`), ArduinoJson's headers (.pio/libdeps after a PlatformIO
build) and git. After regenerating, run `cargo test -p bembeyaz-store` in rust/ and
test/koreader_credentials/check_shim.sh. The corpus only ever holds the C++ behavior. Every user
name, password and server here is made up.

Scenarios: koreader.json files from every config version (1 to 3 and past it), with each setting
missing, in range, out of range or of another type (floats, bools, strings, containers; the shared
value generator, test/json_oracle/oracle.py), obfuscated passwords that decode, do not decode or
are empty, legacy plain passwords, the default, the old default, custom and empty server URLs,
header lists of any length and kind, broken and empty files; every setting set and saved (out of
range too), credentials cleared; edges: roots that are not objects, an empty file, and the longest
string ArduinoJson keeps and one byte more.
"""
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-store/testdata/koreader-credentials-corpus.txt"
SEED = 20261109
# The commit whose store files are the C++ store (before the Rust port).
ORACLE = "4f1317f3"
ORACLE_FILES = ["lib/KOReaderSync/KOReaderCredentialStore.cpp",
                "lib/KOReaderSync/KOReaderCredentialStore.h",
                "lib/Serialization/PersistableStore.cpp", "lib/Serialization/PersistableStore.h",
                "lib/Serialization/StoredPassword.cpp", "lib/Serialization/StoredPassword.h"]
USERS = ["", "okur", "Ayşe", "a\0b", "x" * 40]
PASSWORDS = ["", "parola", "p@ss wörd", "a\0b"]
URLS = ["", "https://sync.crosspointreader.com", "https://sync.koreader.rocks:443",
        "sync.example.org/", "http://192.0.2.1:7200//", "https://kosync.example.net"]
SETTINGS = ["0", "1", "2", "3", "4", "255", "256", "-1", "1.5", "2.9", "-0.5", "300.0", "true",
            "false", '"1"', '"2.5"', '"x"', '""', "null", "[]", "{}", "1e10"]
HEADER_NAMES = ["", "CF-Access-Client-Id", "X-Token"]


def obf(text):
    """The host fake's obfuscation (test/json_oracle/stubs/ObfuscationFake.cpp)."""
    return "~" + text.encode().hex() if text else ""


def text(rng, choices):
    return oracle.esc(rng, rng.choice(choices + oracle.WORDS)) if rng.random() < 0.9 \
        else oracle.value(rng, 1)


def password_fields(rng):
    password = rng.choice(PASSWORDS + oracle.WORDS)
    roll = rng.random()
    if roll < 0.7:
        return {"password_obf": oracle.esc(rng, obf(password))}
    if roll < 0.85:
        return {"password_obf": oracle.esc(rng, rng.choice(["~zz", "plain", ""])),
                "password": oracle.esc(rng, password)}
    if roll < 0.92:
        return {"password": oracle.esc(rng, password)}
    return {"password_obf": oracle.value(rng, 1)}


def headers(rng):
    if rng.random() < 0.1:
        return oracle.value(rng, 1)
    items = []
    for _ in range(rng.choice([0, 1, 2, 2, 3])):
        if rng.random() < 0.1:
            items.append(oracle.value(rng, 1))
            continue
        name = oracle.esc(rng, rng.choice(HEADER_NAMES))
        value = oracle.esc(rng, obf(rng.choice(PASSWORDS))) if rng.random() < 0.8 \
            else oracle.value(rng, 1)
        items.append(f'{{"name":{name},"value_obf":{value}}}')
    return "[" + ",".join(items) + "]"


def setting(rng):
    return rng.choice(SETTINGS) if rng.random() < 0.85 else oracle.value(rng, 1)


def koreader_json(rng):
    fields = {"cfgVersion": setting(rng), "username": text(rng, USERS), "serverUrl": text(rng, URLS),
              "serverType": setting(rng), "matchMethod": setting(rng),
              "sendMetadata": rng.choice(["true", "false", "1", "null", '"true"']),
              "syncClippings": rng.choice(["true", "false", "0", "null"]),
              "syncBehavior": setting(rng), "headers": headers(rng)}
    fields.update(password_fields(rng))
    if rng.random() < 0.3:
        fields["cfgVersion"] = rng.choice(["1", "2", "3"])
    names = [n for n in fields if rng.random() < 0.85]
    if rng.random() < 0.05:
        names.append(rng.choice(oracle.KEYS))
        fields[names[-1]] = oracle.value(rng, 1)
    rng.shuffle(names)
    doc = "{" + ",".join(f'"{n}":{fields[n]}' for n in names) + "}"
    return oracle.broken(rng, doc) if rng.random() < 0.05 else doc


def set_line(rng):
    texts = [rng.choice(USERS), rng.choice(PASSWORDS), rng.choice(URLS)]
    numbers = [rng.choice(["0", "1", "2", "3", "200"]), rng.choice(["0", "1", "2"]),
               str(rng.randrange(2)), str(rng.randrange(2)), rng.choice(["0", "1", "2", "9"])]
    header = [rng.choice(HEADER_NAMES), rng.choice(PASSWORDS), rng.choice(HEADER_NAMES),
              rng.choice(PASSWORDS)]
    return " ".join(["set"] + [oracle.h(t) for t in texts] + numbers + [oracle.h(t) for t in header])


def line(rng):
    roll = rng.random()
    if roll < 0.2:
        return set_line(rng)
    if roll < 0.25:
        return "clear"
    return f"load {oracle.h(koreader_json(rng))}"


def edges():
    docs = ["{}", "[]", "5", "null", "", '{"cfgVersion":1,"username":"u","password":"p"}',
            '{"cfgVersion":1,"username":"u","password":"p","serverUrl":"x"}',
            '{"cfgVersion":3,"serverType":2,"syncBehavior":1}']
    out = [f"load {oracle.h(d)}" for d in docs]
    # ArduinoJson's longest string, 65,535 bytes, and one more, read and written.
    return out + ["long 65535", "long 65536"]


def steps(count):
    rng = random.Random(SEED)
    return "\n".join([line(rng) for _ in range(count)] + edges()) + "\n"


def flags(store_dir):
    return ["-DARDUINOJSON_ENABLE_ARDUINO_STRING=1", f"-I{store_dir}",
            f"-I{oracle.ROOT / 'lib/Serialization'}", f"-I{oracle.ROOT / 'lib/HttpHeader'}"]


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        oracle.check(sys.argv[2], steps(1000), OUT)
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    here = Path(__file__).parent
    with tempfile.TemporaryDirectory() as tmp:
        files = oracle.from_git(tmp, ORACLE, ORACLE_FILES)
        sources = [here / "driver.cpp", oracle.STUBS / "ObfuscationFake.cpp", files[0], files[2],
                   files[4]]
        driver = oracle.build(Path(tmp) / "driver", sources, flags(tmp))
        transcript = oracle.run(driver, steps(count))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(oracle.ROOT)}")


if __name__ == "__main__":
    main()
