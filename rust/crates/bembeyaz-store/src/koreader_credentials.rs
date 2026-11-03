//! `KOReaderCredentialStore`'s JSON (`lib/KOReaderSync/KOReaderCredentialStore.cpp`,
//! `/.crosspoint/koreader.json`): the KOReader sync account (its password obfuscated with the
//! device's key), the server, how it is spoken to and the extra request headers. This reads each
//! setting with its kind as stored, and writes what the store gives, stamped with
//! [`CONFIG_VERSION`]; what the store makes of a file (upgrading an older config, checking each
//! setting, whether to save again) is [`crate::koreader_settings`].

use crate::field::Number;
use crate::headers::{self, Headers};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Writer, or_bool, or_str, parse};

/// The config version every save writes (`CONFIG_VERSION`): bumped when a change to the defaults
/// would change what an existing config does.
pub const CONFIG_VERSION: u8 = 3;

/// What a file holds, for the store to decide on. `password` is a legacy plain password. `S` is
/// the text: views into the parsed file, or owned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stored<S = Vec<u8>> {
    pub username: S,
    pub password_obf: S,
    pub password: S,
    pub server_url: S,
    pub cfg_version: Number,
    pub server_type: Number,
    pub match_method: Number,
    pub sync_behavior: Number,
    /// `| false`, as the store reads them.
    pub send_metadata: bool,
    pub sync_clippings: bool,
    pub headers: Headers<S>,
}

/// What the store saves: the password and the header values obfuscated. `S` is the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved<S> {
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
        byte(w, b"cfgVersion", CONFIG_VERSION);
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

/// [`read_view`]'s settings, copied out (for the tests).
#[cfg(test)]
pub(crate) fn read(text: &[u8]) -> Result<Stored, Error> {
    read_view(text, |s| Stored {
        username: s.username.to_vec(),
        password_obf: s.password_obf.to_vec(),
        password: s.password.to_vec(),
        server_url: s.server_url.to_vec(),
        cfg_version: s.cfg_version,
        server_type: s.server_type,
        match_method: s.match_method,
        sync_behavior: s.sync_behavior,
        send_metadata: s.send_metadata,
        sync_clippings: s.sync_clippings,
        headers: s.headers.owned(),
    })
}

/// Reads `koreader.json`'s text (`deserializeJson`): every setting as stored, its texts read as
/// strings, up to a NUL as C++ reads a `const char*` (a field that is not a string gives empty
/// text), and the first two items of "headers", handed to `use_it` as views into the parsed text,
/// valid during the call.
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; the C++ store then keeps its
/// current settings.
pub fn read_view<R>(text: &[u8], use_it: impl FnOnce(&Stored<&[u8]>) -> R) -> Result<R, Error> {
    let doc = parse(text)?;
    let field = |key: &[u8]| or_str(doc.get(key), b"");
    let number = |key: &[u8]| Number::of(doc.get(key));
    Ok(use_it(&Stored {
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
        headers: headers::view(doc.get(b"headers")),
    }))
}
