"""Builds the Rust static library (rust/) for the selected board and links it.

C++ modules that moved to Rust keep their headers and call the bb_* C ABI
declared under rust/include/. Cargo is incremental, so an unchanged tree adds
about a second to the build.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

Import("env")  # noqa: F821

# MCU -> (Rust target, rustup toolchain, needs build-std)
TARGETS = {
    "esp32s3": ("xtensa-esp32s3-none-elf", "esp", True),
    "esp32c3": ("riscv32imc-unknown-none-elf", None, False),
}

root = Path(env.subst("$PROJECT_DIR"))  # noqa: F821
rust_dir = root / "rust"
mcu = env.BoardConfig().get("build.mcu")  # noqa: F821
if mcu not in TARGETS:
    sys.exit(f"build_rust.py: no Rust target for MCU '{mcu}'")
target, toolchain, build_std = TARGETS[mcu]

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

env.Append(  # noqa: F821
    CPPPATH=[str(rust_dir / "include")],
    LIBPATH=[str(rust_dir / "target" / target / "release")],
    LIBS=["bembeyaz_ffi"],
)
