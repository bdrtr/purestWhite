//! What `KOReaderCredentialStore::fromJsonText` (`lib/KOReaderSync/KOReaderCredentialStore.cpp`)
//! makes of a `koreader.json` file, Rust's alone: the password and header values decoded (the
//! decoder given), an older config upgraded, each setting checked against its range, and whether
//! to save the file again; and the server's base URL (`getBaseUrl`). The C++ firmware calls it
//! through `bembeyaz-ffi`, and the bare-metal firmware can use it as it is. Checked against the
//! store's corpus answers with the host stand-in decoder.

use crate::copied;
use crate::field::Kind;
use crate::headers;
use crate::koreader_credentials::{CONFIG_VERSION, Stored, read_view};
use crate::password::{self, Decoded, Secret};
use alloc::vec::Vec;
use bembeyaz_jsondoc::Error;

/// The default sync server (`crosspoint-sync`, which speaks the whole `KOSync` protocol).
pub const DEFAULT_SERVER_URL: &[u8] = b"https://sync.crosspointreader.com";
/// The default before config version 2; a config from then with credentials and no server URL was
/// syncing here, so it is pinned on upgrade.
pub const LEGACY_DEFAULT_SERVER_URL: &[u8] = b"https://sync.koreader.rocks:443";

/// `KOReaderServerType`: the server's protocol profile.
pub mod server_type {
    /// CrossPoint's protocol extensions, rich position included.
    pub const CROSSPOINT: u8 = 0;
    /// Strict standard `KOSync`.
    pub const KOSYNC: u8 = 1;
    /// A compatible custom server that takes CrossPoint's extensions.
    pub const OTHER: u8 = 2;
}

/// `DocumentMatchMethod`: how a book is matched.
pub mod match_method {
    /// By file name.
    pub const FILENAME: u8 = 0;
    /// By a partial MD5 of the file.
    pub const BINARY: u8 = 1;
}

/// `KOReaderSyncBehavior`: how a manual sync settles a difference.
pub mod sync_behavior {
    /// Always ask.
    pub const ASK_EVERY_TIME: u8 = 0;
    /// Take the furthest progress in simple cases.
    pub const SMART: u8 = 1;
}

/// What a setting's check changed, for the C++ store's debug log: the old default server pinned,
/// and each setting reset because the value read (kept here, never 0) was out of range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    pub pinned_legacy_url: bool,
    pub server_type_reset: Option<u8>,
    pub match_method_reset: Option<u8>,
    pub sync_behavior_reset: Option<u8>,
}

/// The store's settings from a file. `headers` are the file's (at most two); the store keeps what
/// the slots past them held. The plain password and header values are wiped when dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub username: Vec<u8>,
    pub password: Secret,
    pub server_url: Vec<u8>,
    pub server_type: u8,
    pub match_method: u8,
    pub sync_behavior: u8,
    pub send_metadata: bool,
    pub sync_clippings: bool,
    pub headers: Vec<headers::Plain>,
    pub resave: bool,
    pub notes: Notes,
}

impl Settings {
    /// Whether there is an account (`hasCredentials`): a username and a password.
    #[must_use]
    pub fn has_credentials(&self) -> bool {
        !self.username.is_empty() && !self.password.is_empty()
    }
}

/// The settings `koreader.json`'s text gives: [`read_view`], then [`settle`].
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives (the store then keeps its
/// current settings); [`Error::NoMemory`] when memory runs out.
pub fn load(
    text: &[u8],
    decode: impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Settings, Error> {
    read_view(text, |stored| settle(stored, decode)).and_then(|settled| settled)
}

/// The settings `stored` gives, as `fromJsonText` makes them: `decode(text, most)` decodes a stored
/// secret (`obfuscation::deobfuscateFromBase64`).
///
/// # Errors
/// [`Error::NoMemory`] when the decoder or a copy runs out of memory.
pub fn settle<S: AsRef<[u8]>>(
    stored: &Stored<S>,
    decode: impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Settings, Error> {
    let mut resave = false;
    let password = password::stored(
        stored.password_obf.as_ref(),
        stored.password.as_ref(),
        usize::MAX,
        &decode,
        &mut resave,
    )?
    .unwrap_or_default();
    let mut settings = Settings {
        username: copied(stored.username.as_ref())?,
        password,
        server_url: copied(stored.server_url.as_ref())?,
        send_metadata: stored.send_metadata,
        sync_clippings: stored.sync_clippings,
        headers: headers::plain(&stored.headers, &decode)?,
        ..Settings::default()
    };
    settings.match_method = match_method_of(stored, &mut settings.notes);
    upgrade(stored, &mut settings, &mut resave)?;
    settings.sync_behavior = sync_behavior_of(stored, &mut settings.notes, &mut resave);
    settings.resave = resave;
    Ok(settings)
}

/// The upgrades of an older config (formerly C++'s `applyConfigVersion`): before version 2, the old
/// default server pinned for an account with no server URL, and the file saved again either way;
/// before version 3 (or with no server type), the server type worked out from the URL, and saved;
/// else the server type read and checked.
fn upgrade<S: AsRef<[u8]>>(
    stored: &Stored<S>,
    settings: &mut Settings,
    resave: &mut bool,
) -> Result<(), Error> {
    let version = stored.cfg_version.byte_or(1);
    if version < 2 {
        if settings.server_url.is_empty() && settings.has_credentials() {
            settings.server_url = copied(LEGACY_DEFAULT_SERVER_URL)?;
            settings.notes.pinned_legacy_url = true;
        }
        *resave = true;
    }
    let read = stored.server_type;
    settings.server_type = if version < CONFIG_VERSION || read.kind == Kind::Missing {
        *resave = true;
        if is_default(&settings.server_url) {
            server_type::CROSSPOINT
        } else {
            server_type::KOSYNC
        }
    } else if read.byte <= server_type::OTHER {
        read.byte // as<uint8_t>(), whatever the value's type
    } else {
        settings.notes.server_type_reset = Some(read.byte);
        *resave = true;
        server_type::KOSYNC
    };
    Ok(())
}

/// The match method read with `| 0`; one out of range is reset (no resave).
fn match_method_of<S>(stored: &Stored<S>, notes: &mut Notes) -> u8 {
    let method = stored.match_method.byte_or(match_method::FILENAME);
    if method <= match_method::BINARY {
        return method;
    }
    notes.match_method_reset = Some(method);
    match_method::FILENAME
}

/// The sync behavior (formerly C++'s `applySyncBehavior`): read with `| ASK_EVERY_TIME`, a missing
/// one saved, one out of range reset and saved.
fn sync_behavior_of<S>(stored: &Stored<S>, notes: &mut Notes, resave: &mut bool) -> u8 {
    let read = stored.sync_behavior;
    let behavior = read.byte_or(sync_behavior::ASK_EVERY_TIME);
    if behavior <= sync_behavior::SMART {
        *resave |= read.kind == Kind::Missing;
        return behavior;
    }
    notes.sync_behavior_reset = Some(behavior);
    *resave = true;
    sync_behavior::ASK_EVERY_TIME
}

/// The base URL the API paths go after (`getBaseUrl`), as the two pieces it is made of, nothing
/// copied: [`DEFAULT_SERVER_URL`] when there is no server URL, else `http://` put before one with no
/// `://` in it (a local server seldom has TLS), and the trailing slashes taken off the whole.
#[must_use]
pub fn base_url_parts(server_url: &[u8]) -> [&[u8]; 2] {
    let prefix: &'static [u8] = if server_url.is_empty() {
        DEFAULT_SERVER_URL
    } else if server_url.windows(3).any(|w| w == b"://") {
        b""
    } else {
        b"http://"
    };
    let rest = trimmed(server_url);
    if rest.is_empty() {
        return [trimmed(prefix), rest];
    }
    [prefix, rest]
}

/// `text` without its trailing slashes.
fn trimmed(text: &[u8]) -> &[u8] {
    let end = text.iter().rposition(|&b| b != b'/').map_or(0, |at| at + 1);
    text.get(..end).unwrap_or_default()
}

/// Whether `server_url`'s base URL is [`DEFAULT_SERVER_URL`].
fn is_default(server_url: &[u8]) -> bool {
    let [prefix, rest] = base_url_parts(server_url);
    DEFAULT_SERVER_URL
        .strip_prefix(prefix)
        .is_some_and(|tail| tail == rest)
}

/// [`base_url_parts`], joined.
///
/// # Errors
/// [`Error::NoMemory`] when memory runs out.
pub fn base_url(server_url: &[u8]) -> Result<Vec<u8>, Error> {
    let [prefix, rest] = base_url_parts(server_url);
    let mut url = Vec::new();
    url.try_reserve_exact(prefix.len() + rest.len())
        .map_err(|_| Error::NoMemory)?;
    url.extend_from_slice(prefix);
    url.extend_from_slice(rest);
    Ok(url)
}
