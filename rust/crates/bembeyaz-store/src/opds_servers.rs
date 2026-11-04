//! `OpdsServerStore`'s JSON (`src/OpdsServerStore.cpp`, `/.crosspoint/opds.json`): the OPDS
//! catalogs set up, each with its sign-in and up to two extra request headers. The passwords and
//! header values are kept obfuscated with the device's key (the obfuscation is the firmware's).
//! [`load`] makes what the store holds of a file: at most [`MOST_SERVERS`] servers, each password
//! decoded (else a legacy plain one, which asks for the file to be saved again) and each header
//! value decoded;
//! [`write`] writes what the store gives. Adding, changing and removing servers stays in C++.

use crate::copied;
use crate::headers::{self, Headers};
use crate::password::{self, Decoded, Secret};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};

/// How many servers the store keeps (`OpdsServerStore::MAX_SERVERS`).
pub const MOST_SERVERS: usize = 8;

/// A server to save: its password and header values obfuscated. `S` is the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Server<S = Vec<u8>> {
    pub name: S,
    pub url: S,
    pub username: S,
    pub password_obf: S,
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

/// A server as the store holds it: its password and header values plain, wiped when dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plain {
    pub name: Vec<u8>,
    pub url: Vec<u8>,
    pub username: Vec<u8>,
    pub password: Secret,
    /// The file's headers, at most two.
    pub headers: Vec<headers::Plain>,
}

/// The servers a file gives, and whether the store saves it again (a legacy plain password).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    pub servers: Vec<Plain>,
    pub resave: bool,
}

/// What `OpdsServerStore::fromJsonText` makes of `opds.json`'s text, as the C++ store made it
/// before it called this (`addRead`, last in `src/OpdsServerStore.cpp` at c3a2704b): the first
/// [`MOST_SERVERS`] servers, each password read as `obfuscation::readStoredPassword` read it and
/// each header value decoded, `decode(text, most)` decoding (`deobfuscateFromBase64`).
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives (the store then keeps its
/// servers); [`Error::NoMemory`] when the decoder or a copy runs out of memory.
pub fn load(
    text: &[u8],
    decode: impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Loaded, Error> {
    let doc = parse(text)?;
    let list = items(doc.get(b"servers"));
    let mut loaded = Loaded::default();
    loaded
        .servers
        .try_reserve_exact(list.len().min(MOST_SERVERS))
        .map_err(|_| Error::NoMemory)?;
    for item in list.iter().take(MOST_SERVERS) {
        let server = plain(item, &decode, &mut loaded.resave)?;
        loaded.servers.push(server);
    }
    Ok(loaded)
}

/// One server as the store holds it.
fn plain(
    item: &Value,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
    resave: &mut bool,
) -> Result<Plain, Error> {
    let field = |key: &[u8]| or_str(item.get(key), b"");
    let password = password::stored(
        field(b"password_obf"),
        field(b"password"),
        usize::MAX,
        decode,
        resave,
    )?
    .unwrap_or_default();
    Ok(Plain {
        name: copied(field(b"name"))?,
        url: copied(field(b"url"))?,
        username: copied(field(b"username"))?,
        password,
        headers: headers::plain(&headers::view(item.get(b"headers")), decode)?,
    })
}
