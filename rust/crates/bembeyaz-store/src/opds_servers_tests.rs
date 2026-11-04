//! `OpdsServerStore`'s own answers replayed: every step of testdata/opds-servers-corpus.txt
//! (written by `test/opds_servers/make_corpus.py`), its loads made by [`load`] with the host
//! stand-in for the obfuscation (`~` and the text in hex). What stays in C++ is mirrored here as
//! the driver runs it: adding servers within the limit, and the file steps (`loadFromFile`,
//! `saveToFile`).
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, deobfuscate, fuzz, hex, replay};
use crate::headers::{HEADERS, Header, Headers};
use crate::opds_servers::{self, MOST_SERVERS, Server, load, write};
use crate::password::{Decoded, Secret};
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/opds-servers-corpus.txt");
/// The C++ store's `MAX_SERVERS`.
const MOST: usize = MOST_SERVERS;

/// The drivers' stand-in for the obfuscation, as a decoder.
#[allow(clippy::unnecessary_wraps, reason = "a decoder's shape")]
fn stand_in(text: &[u8], _: usize) -> Result<Decoded, crate::Error> {
    Ok(
        deobfuscate(text).map_or_else(Decoded::default, |text| Decoded {
            text: Secret::new(text),
            ok: true,
            too_long: false,
        }),
    )
}

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

impl Store {
    /// A server `load` made, as the driver prints it.
    fn plain(server: &opds_servers::Plain) -> Plain {
        let mut plain = Plain {
            fields: [
                server.name.clone(),
                server.url.clone(),
                server.username.clone(),
                server.password.to_vec(),
            ],
            ..Plain::default()
        };
        for (slot, h) in plain.headers.iter_mut().zip(&server.headers) {
            *slot = (h.name.clone(), h.value.to_vec());
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
        let Ok(loaded) = load(text, stand_in) else {
            return false;
        };
        self.servers = loaded.servers.iter().map(Store::plain).collect();
        let resave = loaded.resave;
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
        let _ = load(input, stand_in);
    });
}

#[test]
fn a_decoder_out_of_memory_is_an_error_not_an_empty_password() {
    let failing = |_: &[u8], _: usize| Err(crate::Error::NoMemory);
    let password = br#"{"servers":[{"name":"a","password_obf":"~61"}]}"#;
    assert_eq!(load(password, failing), Err(crate::Error::NoMemory));
    let header = br#"{"servers":[{"name":"a","headers":[{"name":"X","value_obf":"~61"}]}]}"#;
    let calls = core::cell::Cell::new(0);
    let fails_second = |text: &[u8], most: usize| {
        calls.set(calls.get() + 1);
        if calls.get() < 2 {
            stand_in(text, most)
        } else {
            Err(crate::Error::NoMemory)
        }
    };
    assert_eq!(load(header, fails_second), Err(crate::Error::NoMemory));
}

#[test]
fn only_the_first_eight_servers_are_decoded() {
    let servers: Vec<String> = (0..11)
        .map(|i| format!(r#"{{"name":"s{i}","password_obf":"~61"}}"#))
        .collect();
    let file = format!(r#"{{"servers":[{}]}}"#, servers.join(","));
    let decoded = core::cell::Cell::new(0);
    let counting = |text: &[u8], most: usize| {
        decoded.set(decoded.get() + 1);
        stand_in(text, most)
    };
    let loaded = load(file.as_bytes(), counting).unwrap();
    assert_eq!(loaded.servers.len(), MOST_SERVERS);
    assert_eq!(decoded.get(), MOST_SERVERS);
}
