#!/usr/bin/env python3
"""Builds the app's button-binding rules (driver.cpp over lib/BlePageTurner's BleKeyBinding.h and
the SDK's remote profiles), runs them on seeded random scenarios and writes the transcripts as the
golden corpus purestwhite-ble's tests replay: rust/crates/purestwhite-ble/testdata/binding-corpus.txt.

  test/ble_key_binding/make_corpus.py [CASES]    (default 400; the Rust test wants at least 350)

Needs a host C++20 compiler (`c++`) and ArduinoJson's headers (the version the firmware pins, in
.pio/libdeps after a PlatformIO build). After regenerating, run `cargo test -p purestwhite-ble` in
rust/. The corpus only ever holds the C++ behavior: a deliberate change in the Rust port goes in a
commit of its own, with the cases it changes edited by hand (or regenerated from patched C++).

Each scenario runs in its own process, so every case starts with no saved tables, as each Rust
replay does. Deterministic: the same seed and sources give the same corpus.
"""
import random
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "rust/crates/purestwhite-ble/testdata/binding-corpus.txt"
SEED = 20261103
# ArduinoJson as PlatformIO fetched it for the firmware (platformio.ini pins the version).
JSON = ROOT / ".pio/libdeps/x4pro/ArduinoJson/src"

ADDRS = ["AA:BB:CC:DD:EE:01", "AA:BB:CC:DD:EE:02", "11:22:33:44:55:66", "01:02:03:04:05:06",
         "01:02:03:04:05:06extra", "01:02:03:04:05:0", "", "x", "C0:FF:EE:00:00:01", "C0:FF:EE:00:00:02",
         # C ends these at the NUL: the first is the first address, the second empty.
         "AA:BB:CC:DD:EE:01\0x", "\0AA"]
NAMES = ["BOOX Keypage", "BOOX Keypage V2", "BOOX", "Free3", "Free3-R", "Free", "free3", "Other", ""]
# The profiles' own codes, Free3's on another report id, key identities, a zero value (a release),
# a code with the hold bit set, and the decoder's untied keys.
CODES = [0x030002, 0x030001, 0x030004, 0x030008, 0x000102, 0x000008, 0x010102, 0x7F0008, 0x020004,
         0xFFFF43, 0xFFFF4F, 0x030100, 0x1030004, 0x030010, 0x040201]
ACTIONS = range(9)
USAGES = [0, 0x50, 0x4F, 0x4B, 0x4E, 0x52, 0x51, 0x2C, 0x28, 0x2A, 0xE9, 0xEA, 0xB5, 0xB6, 0x04, 0x29]
HOLDS = [0, 1, 300, 699, 700, 701, 1200, 4000]


def h(text):
    return text.encode().hex() or "-"


def code(rng):
    return rng.choice(CODES) if rng.random() < 0.85 else rng.randrange(1 << 25)


def bits(rng):
    """A slot as read from a card: mostly well formed, sometimes with odd bits."""
    value = (code(rng) & 0xFFFFFF) | rng.choice([0, 0, 1 << 24]) | rng.choice([0, 0, 1 << 25])
    value |= rng.randrange(16) << 28
    if rng.random() < 0.2:
        value |= 1 << rng.randrange(26, 28)
    if rng.random() < 0.1:
        value &= ~0xFF
    return value


def table_step(rng, count, pool, index=None):
    i = rng.randrange(max(count, 1) + 1) if index is None else index
    kind = rng.choice(["learn", "learn", "learn", "clear", "find", "dump"])
    action = rng.choice(ACTIONS)
    if kind == "learn":
        c = code(rng)
        pool.append(c)
        return f"learn {i} {action} {c:x} {rng.choice([0, 1])}"
    return f"{kind} {i} {action}" if kind != "dump" else f"dump {i}"


def ms(t):
    """A time on the 32-bit clock, which wraps."""
    return t & 0xFFFFFFFF


def press(rng, at, out, pool):
    """A press, the waits polled while it is down, and its release (or none)."""
    c = rng.choice(pool) if pool and rng.random() < 0.6 else code(rng)
    legacy = rng.choice([0, 0, 1, 2])
    out.append(f"edge {c:x} 1 {at} {legacy}")
    held = rng.choice(HOLDS)
    for _ in range(rng.choice([0, 1, 2])):
        out.append(f"hold {ms(at + rng.randrange(held + 1))}")
    if rng.random() < 0.85:
        other = c if rng.random() < 0.85 else code(rng)
        out.append(f"edge {other:x} 0 {ms(at + held)} {legacy}")
    out.append(f"hold {ms(at + held + rng.choice([0, 1, 700, 900]))}")
    return ms(at + held + rng.randrange(1, 3000))


def misc_step(rng):
    kind = rng.choice(["valid", "format", "name", "page", "lookup"])
    if kind == "valid":
        return f"valid {bits(rng):x}"
    if kind == "format":
        return f"format {bits(rng):x}"
    if kind == "name":
        return f"name {rng.randrange(16)}"
    if kind == "lookup":
        return f"lookup {code(rng):x} {rng.choice([0, 1])}"
    learned = [0, 0, 0] + USAGES
    return (f"page {rng.choice([0, 1, 1, 1])} {rng.choice(learned):x} {rng.choice(learned):x} "
            f"{rng.choice(USAGES):x} {rng.choice([0, 0, 0, 2, 0x20]):x}")


def focused(rng):
    """One remote's table learned full, then pressed: the hold waits and the full table."""
    addr, name = h(rng.choice(ADDRS[:4])), h(rng.choice(NAMES))
    out, pool, at = [f"edit {addr} {name}"], [], rng.choice([0, 0xFFFFF800])
    if rng.random() < 0.4:
        # Every action on a button of its own: the ninth finds the table full.
        actions = list(ACTIONS)
        rng.shuffle(actions)
        for n, action in enumerate(actions):
            pool.append(0x050001 + n)
            out.append(f"learn 0 {action} {0x050001 + n:x} {rng.choice([0, 1])}")
    for _ in range(rng.randrange(6, 14)):
        if rng.random() < 0.7:
            c = code(rng)
            pool.append(c)
            out.append(f"learn 0 {rng.choice(ACTIONS)} {c:x} {rng.choice([0, 1])}")
        else:
            out.append(table_step(rng, 0, pool, index=0))
    if rng.random() < 0.3:
        # A code with bit 24 set is stored without it, beside the slot it did not match: two
        # slots for one button, and the first one wins.
        first, second = rng.sample([a for a in ACTIONS if a], 2)
        out += [f"learn 0 {first} 30004 0", f"learn 0 {second} 1030004 0"]
        pool += [0x030004, 0x030004]
    out += ["dump 0", f"pick {addr} {name}"]
    for _ in range(rng.randrange(5, 20)):
        at = press(rng, at, out, pool)
    return out


def load(rng):
    """Saved tables as the settings file gives them: up to 5 (one more than kept), each with up to
    10 slots (two more than kept), some of them invalid, some addresses empty or long."""
    tables = []
    for _ in range(rng.randrange(6)):
        slots = [rng.choice([bits(rng), code(rng) | rng.randrange(1, 9) << 28])
                 for _ in range(rng.randrange(11))]
        addr = rng.choice(ADDRS[:-2])
        tables.append(f"{h(addr)} {','.join(f'{b:x}' for b in slots) or '-'}")
    return f"load {len(tables)} {' '.join(tables)}".rstrip()


def scenario(rng):
    if rng.random() < 0.35:
        return focused(rng)
    out, count, at, pool = [], 0, rng.choice([0, 1000, 0xFFFFF000]), []
    for _ in range(rng.randrange(10, 50)):
        roll = rng.random()
        if roll < 0.03:
            out.append(load(rng))
        elif roll < 0.15:
            out.append(f"edit {h(rng.choice(ADDRS))} {h(rng.choice(NAMES))}")
            count = min(count + 1, 4)
        elif roll < 0.2:
            out.append(f"forget {h(rng.choice(ADDRS))}")
        elif roll < 0.45:
            out.append(table_step(rng, count, pool))
        elif roll < 0.55:
            out.append(f"pick {h(rng.choice(ADDRS))} {h(rng.choice(NAMES))}")
        elif roll < 0.8:
            at = press(rng, at, out, pool)
        else:
            out.append(misc_step(rng))
    return out


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 400
    with tempfile.TemporaryDirectory() as tmp:
        driver = Path(tmp) / "driver"
        subprocess.run(["c++", "-std=c++20", "-O1", "-Wall", "-Wextra", "-Werror",
                        f"-I{ROOT}/lib/BlePageTurner/include", f"-isystem{JSON}",
                        f"-I{ROOT}/freeink-sdk/libs/network/BleKeyboardHost/include",
                        str(Path(__file__).with_name("driver.cpp")), "-o", str(driver)], check=True)
        rng = random.Random(SEED)
        corpus = []
        for n in range(cases):
            steps = "\n".join(scenario(rng)) + "\n"
            run = subprocess.run([str(driver)], input=steps, capture_output=True, text=True, check=True)
            corpus.append(f"case {n}\n{run.stdout}")
    OUT.write_text("".join(corpus))
    print(f"{cases} cases, {OUT.stat().st_size} bytes: {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
