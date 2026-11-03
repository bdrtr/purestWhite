#!/usr/bin/env python3
"""Builds driver.cpp over the C++ versions of src/OpdsTokenStore, src/LcpPassphraseStore,
src/network/LcpDeviceStore and lib/Serialization's PersistableStore and StoredPassword (taken from
git, commit d322766b: the stores in the tree are now the Rust shims) with ArduinoJson 7.4.2 and
the host stubs of test/json_oracle, runs it on seeded scenarios and writes the transcript as the
golden corpus the Rust tests replay: rust/crates/bembeyaz-store/testdata/token-stores-corpus.txt.

  test/token_stores/make_corpus.py [STEPS]    (default 700 random steps, and the edge steps)
  test/token_stores/make_corpus.py --check DRIVER

`--check` runs the same steps (the default 700 and the edge steps) through another build of
driver.cpp (over the Rust shims: see check_shim.sh) and says whether its transcript is the corpus,
byte for byte.

Needs a host C++17 compiler (`c++`), ArduinoJson's headers (.pio/libdeps after a PlatformIO
build) and git. After regenerating, run `cargo test -p bembeyaz-store` in rust/ and
test/token_stores/check_shim.sh. The corpus only ever holds the C++ behavior. Every URL, token, key
and device id here is made up.

Scenarios: each store's file with lists of any length (past the stores' 8, which count the entries
they keep), items that are not objects, entries without a URL or provider (skipped), fields of any
type (the shared value generator, test/json_oracle/oracle.py), obfuscated texts that decode, do not
decode, are empty or decode past the stores' limits (2048 token characters, 64 hash and key
characters), a missing or wrong-typed list, broken and empty files; tokens and passphrases put
(new, changed, unchanged, removed, past the limit so the oldest is evicted), the device identity
set; edges: roots that are not objects, an empty file, and the longest string ArduinoJson keeps and
one byte more.
"""
import random
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "json_oracle"))
import oracle  # noqa: E402

OUT = oracle.ROOT / "rust/crates/bembeyaz-store/testdata/token-stores-corpus.txt"
SEED = 20261110
# The commit whose store files are the C++ stores (before the Rust port).
ORACLE = "d322766b"
ORACLE_FILES = ["src/OpdsTokenStore.cpp", "src/OpdsTokenStore.h", "src/LcpPassphraseStore.cpp",
                "src/LcpPassphraseStore.h", "src/network/LcpDeviceStore.cpp",
                "src/network/LcpDeviceStore.h", "lib/Serialization/PersistableStore.cpp",
                "lib/Serialization/PersistableStore.h", "lib/Serialization/StoredPassword.cpp",
                "lib/Serialization/StoredPassword.h"]
URLS = ["", "https://opds.example.org/catalog", "https://lib.example.net/opds", "x", "a\0b"]
PROVIDERS = ["", "https://provider.example.org", "urn:lib:1", "a\0b"]
SECRETS = ["", "tok", "eyJhbGciOi.token.sig", "a" * 64, "a" * 65, "c" * 63]
# Past and at the token limit: rare, as a kept one is written again at every later step.
LONG_SECRETS = ["b" * 2048, "b" * 2049]


def obf(text):
    """The host fake's obfuscation (test/json_oracle/stubs/ObfuscationFake.cpp)."""
    return "~" + text.encode().hex() if text else ""


def obfuscated(rng):
    roll = rng.random()
    if roll < 0.02:
        return oracle.esc(rng, obf(rng.choice(LONG_SECRETS)))
    if roll < 0.75:
        return oracle.esc(rng, obf(rng.choice(SECRETS + oracle.WORDS)))
    if roll < 0.9:
        return oracle.esc(rng, rng.choice(["~zz", "~0", "plain", ""]))
    return oracle.value(rng, 1)


def text(rng, choices):
    return oracle.esc(rng, rng.choice(choices + oracle.WORDS)) if rng.random() < 0.9 \
        else oracle.value(rng, 1)


def item(rng, fields):
    if rng.random() < 0.06:
        return oracle.value(rng, 1)
    names = [n for n in fields if rng.random() < 0.85]
    if rng.random() < 0.05:
        names.append(rng.choice(oracle.KEYS))
    rng.shuffle(names)
    return "{" + ",".join(f'"{n}":{fields.get(n, oracle.value)(rng)}' if n in fields
                          else f'"{n}":{oracle.value(rng, 1)}' for n in names) + "}"


TOKEN_FIELDS = {"url": lambda r: text(r, URLS), "access_obf": obfuscated,
                "refresh_obf": obfuscated, "refresh_url": lambda r: text(r, URLS)}
PASS_FIELDS = {"provider": lambda r: text(r, PROVIDERS), "key_obf": obfuscated}
DEVICE_FIELDS = {"device_id": lambda r: text(r, ["", "dev-1", "a\0b"]), "priv_obf": obfuscated}


def listed(rng, key, fields):
    if rng.random() < 0.05:
        items = oracle.value(rng, 1)
    else:
        n = rng.choice([0, 1, 2, 3, 7, 8, 9, 12])
        items = "[" + ",".join(item(rng, fields) for _ in range(n)) + "]"
    parts = [f'"{key}":{items}'] if rng.random() < 0.95 else []
    return "{" + ",".join(parts) + "}"


def file_of(rng, store):
    if store == "tokens":
        doc = listed(rng, "tokens", TOKEN_FIELDS)
    elif store == "pass":
        doc = listed(rng, "passphrases", PASS_FIELDS)
    else:
        doc = item(rng, DEVICE_FIELDS)
    return oracle.broken(rng, doc) if rng.random() < 0.05 else doc


def put(rng, store):
    h = oracle.h
    if store == "tokens":
        url = rng.choice(URLS[1:] + [f"https://s{i}.example" for i in range(10)])
        access = rng.choice(SECRETS[:3]) if rng.random() < 0.9 else ""
        return f"tokens put {h(url)} {h(access)} {h(rng.choice(SECRETS[:3]))} {h(rng.choice(URLS))}"
    if store == "pass":
        provider = rng.choice(PROVIDERS[1:] + [f"urn:p{i}" for i in range(10)])
        return f"pass put {h(provider)} {h(rng.choice(['', 'a' * 64, 'f00d']))}"
    return f"device set {h(rng.choice(['', 'dev-1', 'dev-2']))} {h(rng.choice(['', 'a' * 64]))}"


def line(rng):
    store = rng.choice(["tokens", "tokens", "pass", "pass", "device"])
    if rng.random() < 0.3:
        return put(rng, store)
    return f"{store} load {oracle.h(file_of(rng, store))}"


def edges():
    out = []
    for store in ["tokens", "pass", "device"]:
        out += [f"{store} load {oracle.h(d)}" for d in ["{}", "[]", "5", "null", ""]]
    # ArduinoJson's longest string, 65,535 bytes, and one more, read and written; then emptied.
    for n in (65535, 65536):
        out += [f"tokens long {n}", f"device long {n}", "tokens load 7b7d", "device load 7b7d"]
    return out


def steps(count):
    rng = random.Random(SEED)
    return "\n".join([line(rng) for _ in range(count)] + edges()) + "\n"


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--check":
        oracle.check(sys.argv[2], steps(700), OUT)
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 700
    here = Path(__file__).parent
    with tempfile.TemporaryDirectory() as tmp:
        files = oracle.from_git(tmp, ORACLE, ORACLE_FILES)
        (Path(tmp) / "network").mkdir()
        (Path(tmp) / "network/LcpDeviceStore.h").write_bytes((Path(tmp) / "LcpDeviceStore.h").read_bytes())
        sources = [here / "driver.cpp", oracle.STUBS / "ObfuscationFake.cpp", files[0], files[2],
                   files[4], files[6], files[8]]
        flags = ["-DARDUINOJSON_ENABLE_ARDUINO_STRING=1", f"-I{tmp}",
                 f"-I{oracle.ROOT / 'lib/Serialization'}"]
        driver = oracle.build(Path(tmp) / "driver", sources, flags)
        transcript = oracle.run(driver, steps(count))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(transcript)
    print(f"{count} random steps and the edge steps, {OUT.stat().st_size} bytes: "
          f"{OUT.relative_to(oracle.ROOT)}")


if __name__ == "__main__":
    main()
