//! C ABI over the Rust modules. Each C++ module that moves to Rust keeps its
//! header and becomes a thin shim over the `bb_*` functions exported here;
//! the matching declarations live in `rust/include/bembeyaz/`.
#![cfg_attr(target_os = "none", no_std)]

extern crate alloc;

mod bidi;
mod bmp;
mod container;
mod css;
#[cfg(target_os = "none")]
mod device;
mod expat;
mod html;
mod hyphenation;
mod jpeg;
mod json;
mod kosync;
mod layout;
mod opf;
mod path;
mod png;
mod png_cover;
mod table;
mod toc;
mod utf8;
mod xml;
mod xtc;
mod zip;
