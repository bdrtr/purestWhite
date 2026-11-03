#!/usr/bin/env python3
"""Builds the SDK's HID report decoder with its fake NimBLE stack (driver.cpp), runs it on
seeded random scenarios shaped like real remotes, and writes the transcripts as the golden corpus
purestwhite-ble's tests replay: rust/crates/purestwhite-ble/testdata/hid-corpus.txt.

  test/ble_hid_decoder/make_corpus.py [CASES]    (default 300; the Rust test wants at least 250)

Needs a host C++17 compiler (`c++`). After regenerating, run `cargo test -p purestwhite-ble` in
rust/. The corpus only ever holds the SDK's behavior: a deliberate change in the Rust port goes in a
commit of its own, with the cases it changes edited by hand (or regenerated from patched C++).

Each scenario runs in its own process, so every case starts from a fresh decoder, as each Rust
replay does. Deterministic: the same seed and SDK give the same corpus.
"""
import random
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SDK = ROOT / "freeink-sdk/libs/network/BleKeyboardHost"
OUT = ROOT / "rust/crates/purestwhite-ble/testdata/hid-corpus.txt"
SEED = 20261103

KEYBOARD_MAP = "05010906a101050719e029e715002501750195088102950175088103190029652565750895068100c0"
CONSUMER_MAP = "050c0901a10185021900" "2aff0326ff03751095018100c0"
BOTH_MAP = ("05010906a1018501050719e029e7150025017501950881029501750881031900296525657508950681"
            "00c0" "050c0901a10185021900" "2aff0326ff03751095018100c0")
VENDOR_MAP = "0600ff0901a1011500" "26ff0075089508810" "2c0"
MAPS = ["-", KEYBOARD_MAP, CONSUMER_MAP, BOTH_MAP, VENDOR_MAP]

KEYS = [0x4F, 0x50, 0x4E, 0x4B, 0x51, 0x52, 0x28, 0x2C, 0x04, 0x1E, 0x01, 0x80, 0xE9]
GAPS = [1, 5, 10, 20, 40, 80, 120, 149, 150, 151, 160, 250, 400, 900]
# Each side of the axis zones' edges (1700 and 2300), and the center.
AXES = [2000, 2000, 1000, 3000, 1699, 1700, 1701, 2299, 2300, 2301, 3500]


def h(data):
    return "".join(f"{b:02x}" for b in data)


def keyboard(rng, shape):
    """A keyboard-shaped frame: 8 bytes, 7 (compact) or 9 (a report id first)."""
    mods = rng.choice([0, 0, 0, 2, 0x20, 1])
    keys = [0] * 6
    for slot in range(rng.choice([0, 0, 1, 1, 1, 2])):
        keys[slot] = rng.choice(KEYS)
    if shape == 7:
        return [mods] + keys
    frame = [mods, 0] + keys
    return [rng.choice([1, 2, 3])] + frame if shape == 9 else frame


def consumer(rng):
    usage = rng.choice([0, 0, 0xE9, 0xEA, 0xB5, 0xB6, 0xCD, 0x223, 0x0])
    return [usage & 0xFF, usage >> 8]


def value_coded(rng, length):
    frame = [0] * length
    if rng.random() < 0.6:
        frame[rng.randrange(length)] = rng.choice([1, 2, 4, 8, 0x10, 0x40, 3])
    return frame


def axis_pad(rng):
    pressed = rng.random() < 0.5
    axes = [rng.choice(AXES) for _ in range(2)]
    status = 0x13 if pressed else 0x12
    return [status, axes[0] & 0xFF, axes[0] >> 8, axes[1] & 0xFF, axes[1] >> 8]


def bitfield(rng, rest, length):
    frame = list(rest)
    if rng.random() < 0.6:
        index = rng.randrange(length)
        frame[index] |= 1 << rng.randrange(8)
    return frame


def frames(rng, model):
    """The frames of one scenario, for one remote model."""
    count = rng.randrange(2, 40)
    length = rng.choice([1, 2, 3, 4, 5, 6])
    rest = [rng.choice([0, 0, 0x10, 0x80]) for _ in range(length)]
    out = []
    held = []
    for _ in range(count):
        if model == "keyboard":
            frame = keyboard(rng, rng.choice([8, 8, 7, 9]))
        elif model == "rollover":
            # One key held while others come and go after it: each key added is one the raw path
            # cannot tie to a byte edge (its first differing byte stays the held key's).
            if not held or rng.random() < 0.1:
                held = [rng.choice(KEYS[:6])]
            elif len(held) < 4 and rng.random() < 0.6:
                held.append(rng.choice([k for k in KEYS[:6] if k not in held]))
            elif len(held) > 1:
                held.pop()
            frame = [0, 0] + held + [0] * (6 - len(held))
        elif model == "swap":
            # One key held for two frames (so its frame is not taken for the rest), then a second
            # key swapped in and out beside it: each swap is a key no byte edge carries.
            first = held[0] if held else rng.choice(KEYS[:6])
            held = [first]
            # Two keys whose bits are not a subset of each other's, so no swap reads as a release.
            second = [] if len(out) < 2 else [(0x28, 0x04)[len(out) % 2]]
            frame = [0, 0] + held + second + [0] * (5 - len(second))
        elif model == "consumer":
            frame = consumer(rng)
        elif model == "value":
            frame = value_coded(rng, length)
        elif model == "axis":
            frame = axis_pad(rng)
        elif model == "bits":
            frame = bitfield(rng, rest, length)
        else:
            frame = [rng.randrange(256) for _ in range(rng.randrange(1, 12))]
        out.append(frame)
    return out


def scenario(rng):
    lines = [f"map {rng.choice(MAPS)}"]
    reports = rng.choice([[0], [3], [1, 2], [0, 3], [4]])
    lines += [f"report {r}" for r in reports]
    model = rng.choice(["keyboard", "rollover", "swap", "consumer", "value", "axis", "bits",
                        "noise"])
    gaps = [5, 10, 20, 60] if model == "swap" else GAPS
    # Most cases take the events after every step; some only now and then, so the queues fill.
    drain_every = rng.choice([1, 1, 3, 8, 40]) if model not in ("rollover", "swap") else rng.choice([1, 6, 40])
    for step, frame in enumerate(frames(rng, model)):
        if rng.random() < 0.25:
            lines.append(f"poll {rng.choice(gaps)}")
        index = 0 if model == "swap" else rng.randrange(len(reports))
        lines.append(f"frame {rng.choice(gaps)} {index} {h(frame)}")
        if step % drain_every == drain_every - 1:
            lines.append("drain")
    lines.append(f"poll {rng.choice(GAPS)}")
    lines.append("drain")
    return "\n".join(lines) + "\n"


def build(out_dir):
    driver = Path(out_dir) / "driver"
    subprocess.run(
        ["c++", "-std=c++17", "-O1", "-w", "-pthread",
         f"-I{SDK}/test/host/stubs", f"-I{SDK}/test/host", f"-I{SDK}/include", f"-I{SDK}/src",
         str(Path(__file__).with_name("driver.cpp")), str(SDK / "test/host/fake_ble.cpp"),
         str(SDK / "src/BleKeyboardHost.cpp"), str(SDK / "src/HidKeymap.cpp"), "-o", str(driver)],
        check=True)
    return driver


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    rng = random.Random(SEED)
    with tempfile.TemporaryDirectory() as tmp:
        driver = build(tmp)
        parts = []
        for case in range(cases):
            script = scenario(rng)
            ran = subprocess.run([str(driver)], input=script, capture_output=True, text=True,
                                 check=True, timeout=30)
            parts.append(f"case {case}\n{ran.stdout}")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("".join(parts))
    print(f"{cases} cases -> {OUT}")


if __name__ == "__main__":
    main()
