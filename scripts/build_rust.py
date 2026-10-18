"""Builds the Rust static library (rust/) for the selected board and links it.

C++ modules that moved to Rust keep their headers and call the bb_* C ABI
declared under rust/include/. Cargo is incremental, so an unchanged tree adds
about a second to the build.
"""

import math
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

Import("env")  # noqa: F821

# MCU -> (Rust target, rustup toolchain, needs build-std, objdump package and name)
TARGETS = {
    "esp32s3": ("xtensa-esp32s3-none-elf", "esp", True, "toolchain-xtensa-esp-elf", "xtensa-esp32s3-elf-objdump"),
    "esp32c3": ("riscv32imc-unknown-none-elf", None, False, "toolchain-riscv32-esp", "riscv32-esp-elf-objdump"),
}

root = Path(env.subst("$PROJECT_DIR"))  # noqa: F821
rust_dir = root / "rust"
mcu = env.BoardConfig().get("build.mcu")  # noqa: F821
if mcu not in TARGETS:
    sys.exit(f"build_rust.py: no Rust target for MCU '{mcu}'")
target, toolchain, build_std, objdump_package, objdump_name = TARGETS[mcu]

cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
command = [cargo]
if toolchain:
    command.append(f"+{toolchain}")
command += ["build", "--release", "-p", "bembeyaz-ffi", "--target", target]
if build_std:
    command.append("-Zbuild-std=core,alloc")

# PlatformIO exports its own compiler variables; keep them away from cargo.
cargo_env = {k: v for k, v in os.environ.items() if k not in ("CC", "CXX", "AR", "CFLAGS", "CXXFLAGS", "LDFLAGS")}
print(f"Rust: building bembeyaz-ffi for {target}")
if subprocess.run(command, cwd=rust_dir, env=cargo_env).returncode != 0:
    sys.exit("build_rust.py: cargo build failed")

# Rust gives no guarantee that a large value is built in place, so a big
# struct (e.g. the ~4.5 KiB inflate state) can silently pass through the stack.
# Refuse frames beyond this; task stacks are 3-24 KiB.
MAX_FRAME = 2048
FUNCTION = re.compile(r"^[0-9a-f]+ <(.+)>:$")
XTENSA_FRAME = re.compile(r"\bentry\s+a1,\s*(0x[0-9a-f]+|\d+)")  # windowed ABI: the whole frame
# Only a prologue moves sp down by a constant, and shrink-wrapping can put it after early
# exits, so each whole function is scanned. LLVM may split a frame into several parts.
RISCV_FRAME = re.compile(r"\baddi\s+sp,\s*sp,\s*-(\d+)")
RISCV_LARGE_FRAME = re.compile(r"\b(?:sub|add)\s+sp,\s*sp,\s*[a-z]")  # parts over 2047 bytes


def functions(listing):
    """Yields (name, instructions) per function of an objdump listing."""
    name, body = None, []
    for line in listing.splitlines():
        if match := FUNCTION.match(line):
            if name:
                yield name, body
            name, body = match.group(1), []
        elif name and line.strip():
            body.append(line)
    if name:
        yield name, body


def frame_size(body):
    """Bytes the prologue reserves (inf when a register holds the amount), or None without one."""
    frame = None
    for line in body:
        if match := XTENSA_FRAME.search(line):
            return int(match.group(1), 0)
        if match := RISCV_FRAME.search(line):
            frame = (frame or 0) + int(match.group(1))
        elif RISCV_LARGE_FRAME.search(line):
            return math.inf
    return frame


def large_frames(objdump, library):
    """Yields (function, frame size) for prologues over MAX_FRAME."""
    listing = subprocess.run([objdump, "-d", "-C", "--no-show-raw-insn", library], capture_output=True, text=True)
    if listing.returncode != 0:
        sys.exit(f"build_rust.py: {objdump} failed: {listing.stderr.strip()}")
    prologues = 0
    for function, body in functions(listing.stdout):
        frame = frame_size(body)
        if frame is None:
            continue
        prologues += 1
        if frame > MAX_FRAME:
            yield function, "over 4 KiB" if frame == math.inf else f"{frame} bytes"
    if prologues == 0:
        sys.exit(f"build_rust.py: found no function prologues in {library}; is {objdump} the right objdump?")


library = rust_dir / "target" / target / "release" / "libbembeyaz_ffi.a"
objdump_dir = env.PioPlatform().get_package_dir(objdump_package)  # noqa: F821
if not objdump_dir:
    sys.exit(f"build_rust.py: PlatformIO package {objdump_package} is not installed")
objdump = Path(objdump_dir) / "bin" / objdump_name
oversized = list(large_frames(str(objdump), library))
if oversized:
    for function, frame in oversized:
        print(f"build_rust.py: {function} has a stack frame of {frame} (limit {MAX_FRAME})")
    sys.exit("build_rust.py: Rust stack frame too large; build the value in place")

env.Append(  # noqa: F821
    CPPPATH=[str(rust_dir / "include")],
    LIBPATH=[str(rust_dir / "target" / target / "release")],
    LIBS=["bembeyaz_ffi"],
)
