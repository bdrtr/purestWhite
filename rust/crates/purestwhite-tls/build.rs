//! Builds wolfSSL (rust/third_party/wolfssl, a submodule at v5.7.2-stable, the release the C++
//! firmware uses) as a static library with `c/user_settings.h`: for the ESP32-S3 with Espressif's
//! GCC, or for the host the tests run on. The server is built only with the `test-server` feature.
//!
//! On the ESP32-S3 the C library wolfSSL calls is the chip's ROM (memcpy, strcmp, strstr and the
//! like, which esp-hal's linker scripts name), with the two pieces the ROM lacks: `strlen`
//! (`c/rom_gaps.c`) and the `_ctype_` table, taken from Espressif's newlib. Not the whole newlib:
//! its stack-protector object would clash with esp-hal's, and its assembly `strlen` keeps literals
//! the linker cannot reach in esp-hal's layout.

use std::path::{Path, PathBuf};

/// wolfSSL's TLS sources; `ssl.c` includes the other `ssl_*.c` files itself.
const TLS: &[&str] = &[
    "internal.c",
    "keys.c",
    "ssl.c",
    "tls.c",
    "tls13.c",
    "wolfio.c",
];

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let wolfssl = crate_dir.join("../../third_party/wolfssl");
    assert!(
        wolfssl.join("wolfssl/ssl.h").exists(),
        "wolfSSL is missing: run `git submodule update --init rust/third_party/wolfssl`"
    );
    let target = std::env::var("TARGET").expect("set by cargo");
    let mut build = cc::Build::new();
    build
        .include(crate_dir.join("c"))
        .include(&wolfssl)
        .define("WOLFSSL_USER_SETTINGS", None)
        .warnings(false)
        .file(crate_dir.join("c/shim.c"));
    for file in TLS {
        build.file(wolfssl.join("src").join(file));
    }
    for file in crypto_sources(&wolfssl) {
        build.file(file);
    }
    if std::env::var_os("CARGO_FEATURE_TEST_SERVER").is_some() {
        build.define("PURESTWHITE_TLS_TEST_SERVER", None);
    }
    if target.starts_with("xtensa-esp32s3") {
        build
            .compiler("xtensa-esp32s3-elf-gcc")
            .archiver("xtensa-esp32s3-elf-ar")
            .flag("-mlongcalls")
            .flag("-ffunction-sections")
            .flag("-fdata-sections")
            .opt_level_str("s");
        build.file(crate_dir.join("c/rom_gaps.c"));
        build.object(newlib_ctype());
    }
    build.compile("wolfssl");
    println!("cargo:rerun-if-changed=c");
    println!("cargo:rerun-if-changed=build.rs");
    // A new wolfSSL (the submodule moved) is built again.
    for dir in ["src", "wolfcrypt/src", "wolfssl"] {
        println!("cargo:rerun-if-changed={}", wolfssl.join(dir).display());
    }
}

/// wolfCrypt's sources: every `.c` at the top of `wolfcrypt/src`. Each is guarded by the features
/// it serves, so those `user_settings.h` leaves out compile to nothing.
fn crypto_sources(wolfssl: &Path) -> Vec<PathBuf> {
    let dir = wolfssl.join("wolfcrypt/src");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("wolfcrypt/src")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "c"))
        .collect();
    files.sort();
    files
}

/// newlib's `_ctype_` table (the ctype macros' flags) for the ESP32-S3, taken out of its `libc.a`
/// (found beside the compiler) into `OUT_DIR`.
fn newlib_ctype() -> PathBuf {
    const MEMBER: &str = "libc_a-ctype_.o";
    let output = std::process::Command::new("xtensa-esp32s3-elf-gcc")
        .arg("-print-file-name=libc.a")
        .output()
        .expect("xtensa-esp32s3-elf-gcc");
    let libc = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo"));
    let status = std::process::Command::new("xtensa-esp32s3-elf-ar")
        .arg("x")
        .arg(&libc)
        .arg(MEMBER)
        .current_dir(&out)
        .status()
        .expect("xtensa-esp32s3-elf-ar");
    assert!(status.success(), "{} lacks {MEMBER}", libc.display());
    out.join(MEMBER)
}
