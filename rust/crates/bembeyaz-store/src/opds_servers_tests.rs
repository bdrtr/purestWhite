//! `OpdsServerStore`'s own answers replayed: every step of testdata/opds-servers-corpus.txt
//! (written by `test/opds_servers/make_corpus.py`). What stays in C++ is mirrored here as the
//! driver runs it: the host stand-in for the obfuscation (`~` and the text in hex), the legacy
//! password fallback and resave (`obfuscation::readStoredPassword`), the 8-server limit, and the
//! file steps (`loadFromFile`, `saveToFile`).
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, fuzz, hex, replay};
use crate::headers::{HEADERS, Header, Headers};
use crate::opds_servers::{Server, read, write};
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/opds-servers-corpus.txt");
/// The C++ store's `MAX_SERVERS`.
const MOST: usize = 8;

/// A server as the C++ store holds it: plain password and header values.
#[derive(Clone, Default)]
struct Plain {
    fields: [Vec<u8>; 4],
    headers: [(Vec<u8>, Vec<u8>); HEADERS],
}

#[derive(Default)]
struct Store {
    servers: Vec<Plain>,
    written: Vec<u8>,
}

fn obfuscate(plain: &[u8]) -> Vec<u8> {
    if plain.is_empty() {
        return Vec::new();
    }
    format!("~{}", hex(plain)).into_bytes()
}

/// The stand-in's decoding: the text, or none when it does not decode.
fn deobfuscate(text: &[u8]) -> Option<Vec<u8>> {
    let digits = text.strip_prefix(b"~")?;
    if digits.len() % 2 != 0 {
        return None;
    }
    let digits = core::str::from_utf8(digits).ok()?;
    (0..digits.len() / 2)
        .map(|i| u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).ok())
        .collect()
}

impl Store {
    fn plain(server: &Server, resave: &mut bool) -> Plain {
        let password = deobfuscate(&server.password_obf).unwrap_or_else(|| {
            *resave |= !server.password.is_empty();
            server.password.clone()
        });
        let mut plain = Plain {
            fields: [
                server.name.clone(),
                server.url.clone(),
                server.username.clone(),
                password,
            ],
            ..Plain::default()
        };
        let stored = &server.headers.slots[..server.headers.count];
        for (slot, h) in plain.headers.iter_mut().zip(stored) {
            *slot = (
                h.name.clone(),
                deobfuscate(&h.value_obf).unwrap_or_default(),
            );
        }
        plain
    }

    fn save(&mut self) {
        let stored: Vec<Server> = self
            .servers
            .iter()
            .map(|p| Server {
                name: p.fields[0].clone(),
                url: p.fields[1].clone(),
                username: p.fields[2].clone(),
                password_obf: obfuscate(&p.fields[3]),
                password: Vec::new(),
                headers: Headers {
                    slots: p.headers.clone().map(|(name, value)| Header {
                        name,
                        value_obf: obfuscate(&value),
                    }),
                    count: HEADERS,
                },
            })
            .collect();
        self.written = write(&stored).unwrap();
    }

    fn load(&mut self, file: &[u8]) -> bool {
        self.written.clear();
        let text = file.split(|b| *b == 0).next().unwrap_or_default();
        if text.is_empty() {
            return false;
        }
        let Ok(stored) = read(text, MOST) else {
            return false;
        };
        let mut resave = false;
        self.servers = stored
            .iter()
            .map(|s| Store::plain(s, &mut resave))
            .collect();
        if resave {
            self.save();
        }
        true
    }

    fn add(&mut self, server: Plain) -> bool {
        self.written.clear();
        if self.servers.len() >= MOST {
            return false;
        }
        self.servers.push(server);
        self.save();
        true
    }

    fn answer(&self, ok: bool) -> String {
        let mut out = format!("{} {}", if ok { "ok" } else { "fail" }, self.servers.len());
        for p in &self.servers {
            let mut fields: Vec<String> = p.fields.iter().map(|f| hex(f)).collect();
            for (name, value) in &p.headers {
                fields.push(hex(name));
                fields.push(hex(value));
            }
            out.push(' ');
            out.push_str(&fields.join(","));
        }
        let written = if self.written.is_empty() {
            "-".into()
        } else {
            String::from_utf8(self.written.clone()).unwrap()
        };
        format!("{out} | {written}")
    }

    fn lengths(&self, ok: bool) -> String {
        let first = self.servers.first().map_or(0, |s| s.fields[0].len());
        let ok = if ok { "ok" } else { "fail" };
        format!("{ok} {} {first} {}", self.servers.len(), self.written.len())
    }
}

fn step(store: &mut Store, a: &[&str]) -> String {
    match a[0] {
        "load" => {
            let ok = store.load(&bytes(a[1]));
            store.answer(ok)
        }
        "add" => {
            let f: Vec<Vec<u8>> = a[1..9].iter().map(|h| bytes(h)).collect();
            let server = Plain {
                fields: [f[0].clone(), f[1].clone(), f[2].clone(), f[3].clone()],
                headers: [(f[4].clone(), f[5].clone()), (f[6].clone(), f[7].clone())],
            };
            let ok = store.add(server);
            store.answer(ok)
        }
        "long" => {
            let n: usize = a[1].parse().unwrap();
            let name = "n".repeat(n);
            let loaded =
                store.load(format!("{{\"servers\":[{{\"name\":\"{name}\"}}]}}").as_bytes());
            let first = store.lengths(loaded);
            let server = Plain {
                fields: [name.into_bytes(), Vec::new(), Vec::new(), Vec::new()],
                ..Plain::default()
            };
            let added = store.add(server);
            format!("{first} | {}", store.lengths(added))
        }
        other => panic!("unknown step {other}"),
    }
}

#[test]
fn every_step_of_the_corpus_is_the_stores() {
    let mut store = Store::default();
    replay(CORPUS, 1000, |a| step(&mut store, a));
}

#[test]
fn broken_opds_files_are_read_without_a_fault() {
    fuzz(CORPUS, 20, |input| {
        let _ = read(input, MOST);
    });
}
