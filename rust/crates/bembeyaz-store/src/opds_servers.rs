//! `OpdsServerStore`'s JSON (`src/OpdsServerStore.cpp`, `/.crosspoint/opds.json`): the OPDS
//! catalogs set up, each with its sign-in and up to two extra request headers. The passwords and
//! header values are kept obfuscated with the device's key; that, and turning a legacy plain
//! password into an obfuscated one, stays in C++, so this reads and writes their stored texts as
//! they are. Adding, changing and removing servers stays in C++ too, which also keeps how many
//! servers the list holds (`MAX_SERVERS`).

use crate::headers::{self, Headers};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};

/// A server as stored. `password` is a legacy plain password: read when a file still has one,
/// never written. `S` is the text: owned when read, borrowed (or owned) when written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Server<S = Vec<u8>> {
    pub name: S,
    pub url: S,
    pub username: S,
    pub password_obf: S,
    pub password: S,
    pub headers: Headers<S>,
}

/// Writes `opds.json`'s text for `servers` (`toJson`, then `serializeJson`); none when memory ran
/// out.
#[must_use]
pub fn write<S: AsRef<[u8]>>(servers: &[Server<S>]) -> Option<Vec<u8>> {
    let size: usize = servers.iter().map(|s| 160 + text_size(s)).sum();
    let mut w = Writer::with_capacity(16 + size);
    w.object(|w| {
        w.key(b"servers");
        w.array(|w| servers.iter().for_each(|server| write_server(w, server)));
    });
    w.finish()
}

fn text_size<S: AsRef<[u8]>>(s: &Server<S>) -> usize {
    let texts = [&s.name, &s.url, &s.username, &s.password_obf].map(|t| t.as_ref().len());
    texts.iter().sum::<usize>() + s.headers.text_size()
}

fn write_server<S: AsRef<[u8]>>(w: &mut Writer, server: &Server<S>) {
    w.object(|w| {
        w.key(b"name");
        w.string(server.name.as_ref());
        w.key(b"url");
        w.string(server.url.as_ref());
        w.key(b"username");
        w.string(server.username.as_ref());
        w.key(b"password_obf");
        w.string(server.password_obf.as_ref());
        w.key(b"headers");
        server.headers.write(w);
    });
}

/// Reads `opds.json`'s text (`deserializeJson`, then `fromJson`): the first `most` items of its
/// "servers" array (none when it has no such array), and of each the first two items of its
/// "headers" array; every field read as a string, up to a NUL as C++ reads a `const char*` (an
/// item that is not an object, or a field that is not a string, gives empty text).
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; the C++ store then keeps its
/// current servers.
pub fn read(text: &[u8], most: usize) -> Result<Vec<Server>, Error> {
    let doc = parse(text)?;
    Ok(items(doc.get(b"servers"))
        .iter()
        .take(most)
        .map(read_server)
        .collect())
}

fn field(item: &Value, key: &[u8]) -> Vec<u8> {
    or_str(item.get(key), b"").to_vec()
}

fn read_server(item: &Value) -> Server {
    Server {
        name: field(item, b"name"),
        url: field(item, b"url"),
        username: field(item, b"username"),
        password_obf: field(item, b"password_obf"),
        password: field(item, b"password"),
        headers: headers::read(item.get(b"headers")),
    }
}
