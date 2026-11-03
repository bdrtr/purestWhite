//! `KOReaderCredentialStore`'s JSON (`lib/KOReaderSync/KOReaderCredentialStore.cpp`,
//! `/.crosspoint/koreader.json`): the KOReader sync account (its password obfuscated with the
//! device's key), the server, how it is spoken to and the extra request headers. Upgrading an
//! older config, checking each setting, and deciding whether to save again stay in C++, so this
//! reads each setting with its kind as stored, and writes what C++ gives.

use crate::field::Number;
use crate::headers::{self, Headers};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Writer, or_bool, or_str, parse};

/// What a file holds, for C++ to decide on. `password` is a legacy plain password.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stored {
    pub username: Vec<u8>,
    pub password_obf: Vec<u8>,
    pub password: Vec<u8>,
    pub server_url: Vec<u8>,
    pub cfg_version: Number,
    pub server_type: Number,
    pub match_method: Number,
    pub sync_behavior: Number,
    /// `| false`, as the store reads them.
    pub send_metadata: bool,
    pub sync_clippings: bool,
    pub headers: Headers,
}

/// What C++ saves: the password and the header values obfuscated. `S` is the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved<S> {
    pub cfg_version: u8,
    pub username: S,
    pub password_obf: S,
    pub server_url: S,
    pub server_type: u8,
    pub match_method: u8,
    pub send_metadata: bool,
    pub sync_clippings: bool,
    pub sync_behavior: u8,
    pub headers: Headers<S>,
}

/// Writes `koreader.json`'s text (`toJson`, then `serializeJson`); none when memory ran out.
#[must_use]
pub fn write<S: AsRef<[u8]>>(saved: &Saved<S>) -> Option<Vec<u8>> {
    let texts = [&saved.username, &saved.password_obf, &saved.server_url].map(|t| t.as_ref().len());
    let size = texts.iter().sum::<usize>() + saved.headers.text_size();
    let mut w = Writer::with_capacity(256 + size);
    let byte = |w: &mut Writer, key: &[u8], n: u8| {
        w.key(key);
        w.unsigned(n.into());
    };
    let text = |w: &mut Writer, key: &[u8], t: &S| {
        w.key(key);
        w.string(t.as_ref());
    };
    w.object(|w| {
        byte(w, b"cfgVersion", saved.cfg_version);
        text(w, b"username", &saved.username);
        text(w, b"password_obf", &saved.password_obf);
        text(w, b"serverUrl", &saved.server_url);
        byte(w, b"serverType", saved.server_type);
        byte(w, b"matchMethod", saved.match_method);
        w.key(b"sendMetadata");
        w.bool(saved.send_metadata);
        w.key(b"syncClippings");
        w.bool(saved.sync_clippings);
        byte(w, b"syncBehavior", saved.sync_behavior);
        w.key(b"headers");
        saved.headers.write(w);
    });
    w.finish()
}

/// Reads `koreader.json`'s text (`deserializeJson`): every setting as stored, its texts read as
/// strings, up to a NUL as C++ reads a `const char*` (a field that is not a string gives empty
/// text), and the first two items of "headers".
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; the C++ store then keeps its
/// current settings.
pub fn read(text: &[u8]) -> Result<Stored, Error> {
    let doc = parse(text)?;
    let field = |key: &[u8]| or_str(doc.get(key), b"").to_vec();
    let number = |key: &[u8]| Number::of(doc.get(key));
    Ok(Stored {
        username: field(b"username"),
        password_obf: field(b"password_obf"),
        password: field(b"password"),
        server_url: field(b"serverUrl"),
        cfg_version: number(b"cfgVersion"),
        server_type: number(b"serverType"),
        match_method: number(b"matchMethod"),
        sync_behavior: number(b"syncBehavior"),
        send_metadata: or_bool(doc.get(b"sendMetadata"), false),
        sync_clippings: or_bool(doc.get(b"syncClippings"), false),
        headers: headers::read(doc.get(b"headers")),
    })
}
