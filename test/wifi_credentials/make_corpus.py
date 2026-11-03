#!/usr/bin/env python3
"""Builds driver.cpp over the C++ versions of src/WifiCredentialStore, lib/Serialization's
PersistableStore and StoredPassword (taken from git, commit 6615f58d: the store in the tree is now
the Rust shim) with ArduinoJson 7.4.2 and the host stubs of test/json_oracle, runs it on seeded
scenarios and writes the transcript as the golden corpus the Rust tests replay:
rust/crates/bembeyaz-store/testdata/wifi-credentials-corpus.txt.

  test/wifi_credentials/make_corpus.py [STEPS]    (default 1000 random steps, and the edge steps)
  test/wifi_credentials/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 1000 and the edge steps) through another build of
driver.cpp (over the Rust shim: see check_shim.sh) and says whether its transcript is the corpus,
byte for byte.

Needs a host C++17 compiler (`c++`), ArduinoJson's headers (.pio/libdeps after a PlatformIO
build) and git. After regenerating, run `cargo test -p bembeyaz-store` in rust/ and
test/wifi_credentials/check_shim.sh. The corpus only ever holds the C++ behavior. Every SSID and
password here is made up.

Scenarios: wifi.json files with credential lists of any length (past the store's 8, which counts the
networks it keeps), items that are not objects, fields of any type (the shared value generator,
test/json_oracle/oracle.py), obfuscated passwords that decode, do not decode or are empty, legacy
plain passwords, lengths and CRC-32s that match, do not match, are missing, are of another type or
out of range, a missing or wrong-typed "credentials", broken and empty files; networks added (past
8, and passwords longer than the 64 bytes the store accepts when it reads a file), the last
connected network set, networks removed; edges: roots that are not objects, a "credentials" that is
not an array, an empty file, a legacy plain password alone, an empty password with a zero length and
CRC-32, and the longest string ArduinoJson keeps and one byte more.
"""
import random
import sys
import tempfile
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-store/testdata/wifi-credentials-corpus.txt"
SEED = 20261108
# The commit whose store files are the C++ store (before the Rust port).
ORACLE = "6615f58d"
ORACLE_FILES = ["src/WifiCredentialStore.cpp", "src/WifiCredentialStore.h",
                "lib/Serialization/PersistableStore.cpp", "lib/Serialization/PersistableStore.h",
                "lib/Serialization/StoredPassword.cpp", "lib/Serialization/StoredPassword.h"]
SSIDS = ["", "Ev", "Kafe Çınar", "net-5G", "日本", "a\0b", "x" * 32]
PASSWORDS = ["", "parola123", "p@ss wörd", "x" * 63, "x" * 64, "x" * 65, "a\0b", "12345678"]


def obf(text):
    """The host fake's obfuscation (test/json_oracle/stubs/ObfuscationFake.cpp)."""
    return "~" + text.encode().hex() if text else ""


def crc(text):
    return zlib.crc32(text.encode())


def number(rng, right, wide=True):
    """A password_len or password_crc32 value: mostly `right`, else off, of another type, out of
    range or null. Without `wide`, no integer past uint32_t: the store reads password_len as a
    size_t, 64 bits on the host this runs on but 32 on the device, so the two would judge such a
    length differently (the networks kept are the same, the log line is not)."""
    roll = rng.random()
    if roll < 0.6:
        return str(right)
    wide_ones = ["4294967296", "18446744073709551615"] if wide else []
    return rng.choice([str(right + 1), "-1", "0", "65", "4294967295"] + wide_ones +
                      ["1.5", f"{right}.0", f'"{right}"', "true", "null", "[]", "{}"])


def credential(rng):
    if rng.random() < 0.06:
        return oracle.value(rng, 1)
    password = rng.choice(PASSWORDS + oracle.WORDS)
    fields = {"ssid": oracle.esc(rng, rng.choice(SSIDS + oracle.WORDS))}
    roll = rng.random()
    if roll < 0.75:
        fields["password_obf"] = oracle.esc(rng, obf(password))
    elif roll < 0.85:
        fields["password_obf"] = oracle.esc(rng, rng.choice(["~zz", "~0", "plain", ""]))
        fields["password"] = oracle.esc(rng, password)
    elif roll < 0.9:
        fields["password"] = oracle.esc(rng, password)
    else:
        fields["password_obf"] = oracle.value(rng, 1)
    if rng.random() < 0.85:
        fields["password_len"] = number(rng, len(password.encode()), wide=False)
    if rng.random() < 0.8:
        fields["password_crc32"] = number(rng, crc(password))
    names = [n for n in fields if rng.random() < 0.95]
    if rng.random() < 0.05:
        names.append(rng.choice(oracle.KEYS))
        fields[names[-1]] = oracle.value(rng, 1)
    rng.shuffle(names)
    return "{" + ",".join(f'"{n}":{fields[n]}' for n in names) + "}"


def wifi_json(rng):
    parts = []
    if rng.random() < 0.8:
        value = (oracle.esc(rng, rng.choice(SSIDS + oracle.WORDS)) if rng.random() < 0.85
                 else oracle.value(rng, 1))
        parts.append(f'"lastConnectedSsid":{value}')
    if rng.random() < 0.95:
        if rng.random() < 0.05:
            items = oracle.value(rng, 1)
        else:
            n = rng.choice([0, 1, 2, 3, 7, 8, 9, 12])
            items = "[" + ",".join(credential(rng) for _ in range(n)) + "]"
        parts.append(f'"credentials":{items}')
    rng.shuffle(parts)
    text = "{" + ",".join(parts) + "}"
    if rng.random() < 0.05:
        text = oracle.broken(rng, text)
    return text


def line(rng):
    roll = rng.random()
    if roll < 0.15:
        return f"add {oracle.h(rng.choice(SSIDS + oracle.WORDS))} {oracle.h(rng.choice(PASSWORDS))}"
    if roll < 0.2:
        return f"last {oracle.h(rng.choice(SSIDS))}"
    if roll < 0.25:
        return f"remove {oracle.h(rng.choice(SSIDS))}"
    return f"load {oracle.h(wifi_json(rng))}"


def edges():
    docs = ["{}", "[]", "5", "null", '{"credentials":{}}', '{"credentials":null}', "",
            '{"credentials":[{"ssid":"a","password":"legacy"}]}',
            '{"credentials":[{"ssid":"a","password_obf":"","password_len":0,"password_crc32":0}]}']
    out = [f"load {oracle.h(d)}" for d in docs]
    # ArduinoJson's longest string, 65,535 bytes, and one more, read and written.
    return out + ["long 65535", "long 65536"]


def steps(count):
    rng = random.Random(SEED)
    return "\n".join([line(rng) for _ in range(count)] + edges()) + "\n"


def flags(store_dir):
    return ["-DARDUINOJSON_ENABLE_ARDUINO_STRING=1", "-DSTORE_LOG_ERRORS", f"-I{store_dir}",
            f"-I{oracle.ROOT / 'lib/Serialization'}", f"-I{oracle.ROOT / 'src'}"]


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
