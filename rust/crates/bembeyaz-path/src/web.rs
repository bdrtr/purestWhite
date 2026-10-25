//! Paths that arrive over the network (the web file manager, `WebDAV`, plugin endpoints): which
//! ones the card's system folders and credential stores keep out of reach, and how a request's
//! path becomes the absolute path the handlers open.

use crate::normalise;

/// Folders the web interfaces never list or touch, beside names starting with a dot.
pub const SYSTEM_NAMES: [&[u8]; 2] = [b"System Volume Information", b"XTCache"];

/// The credential stores no web request or plugin may read or write: their passwords are
/// obfuscated with a device key. Matched as prefixes, so the stores' temporary and backup
/// siblings are covered too.
pub const SENSITIVE_FILES: [&[u8]; 3] = [
    b"/.crosspoint/wifi.json",
    b"/.crosspoint/opds.json",
    b"/.crosspoint/koreader.json",
];

/// `bytes` up to its first NUL, as C reads it.
fn c_str(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or_default()
}

/// One of [`SYSTEM_NAMES`], up to the first NUL: a decoded `%00` ends the name `SdFat` creates, so
/// `XTCache\0x` is `XTCache`.
#[must_use]
pub fn is_system_name(name: &[u8]) -> bool {
    SYSTEM_NAMES.contains(&c_str(name))
}

/// A name starting with a dot or a system folder.
#[must_use]
pub fn is_hidden_name(name: &[u8]) -> bool {
    name.first() == Some(&b'.') || is_system_name(name)
}

/// Whether any component of `path` is [hidden](is_hidden_name); the whole of `path` is searched,
/// NULs included, and each component is a name up to its first NUL.
#[must_use]
pub fn has_hidden_component(path: &[u8]) -> bool {
    path.split(|&b| b == b'/').any(is_hidden_name)
}

/// `SdFat` also opens a name by its generated 8.3 alias (`.crosspoint` is `CROSSP~1`), which no
/// spelling check can map back to the long name: a `~` and a digit, at most 8 bytes before the
/// first dot and 3 after it.
fn is_short_alias(name: &[u8]) -> bool {
    let tilde_digit = name
        .iter()
        .position(|&b| b == b'~')
        .and_then(|at| name.get(at + 1))
        .is_some_and(u8::is_ascii_digit);
    if !tilde_digit {
        return false;
    }
    let (base, ext) = match name.iter().position(|&b| b == b'.') {
        Some(dot) => (dot, name.len() - dot - 1),
        None => (name.len(), 0),
    };
    base <= 8 && ext <= 3
}

/// The name `SdFat` would open for `name`: leading spaces skipped, trailing dots and spaces dropped.
fn opened_name(name: &[u8]) -> &[u8] {
    let start = name.iter().position(|&b| b != b' ').unwrap_or(name.len());
    let name = name.get(start..).unwrap_or_default();
    let end = name
        .iter()
        .rposition(|&b| b != b'.' && b != b' ')
        .map_or(0, |at| at + 1);
    name.get(..end).unwrap_or_default()
}

/// Whether `path` (up to a NUL) reaches a [credential store](SENSITIVE_FILES), however it is
/// spelled: `//`, `.` and `..` are resolved first, case is ignored, names are compared as `SdFat`
/// opens them, and an 8.3 alias in the first two components is refused outright.
#[must_use]
pub fn is_sensitive(path: &[u8]) -> bool {
    // The stores sit two components deep, so only the first two of the resolved path decide.
    let mut parts: [&[u8]; 2] = [&[], &[]];
    let mut depth = 0usize;
    for component in c_str(path).split(|&b| b == b'/').filter(|c| !c.is_empty()) {
        if component == b".." {
            depth = depth.saturating_sub(1);
            continue;
        }
        let name = opened_name(component);
        if depth < 2 && is_short_alias(name) {
            return true;
        }
        if name.is_empty() {
            continue;
        }
        if let Some(part) = parts.get_mut(depth) {
            *part = name;
        }
        depth += 1;
    }
    // A path of one component is shorter than every store.
    if depth < 2 {
        return false;
    }
    let [first, second] = parts;
    let resolved: [&[u8]; 4] = [b"/", first, b"/", second];
    SENSITIVE_FILES
        .iter()
        .any(|file| starts_with_ignoring_case(&resolved, file))
}

/// Whether the joined `parts` start with `prefix`, ASCII case ignored.
fn starts_with_ignoring_case(parts: &[&[u8]], prefix: &[u8]) -> bool {
    let mut bytes = parts.iter().flat_map(|p| p.iter());
    prefix
        .iter()
        .all(|p| bytes.next().is_some_and(|b| b.eq_ignore_ascii_case(p)))
}

/// A path a plugin may name (in a web request or its manifest): absolute, not `/` alone, without
/// `..` anywhere, and not a [credential store](is_sensitive).
#[must_use]
pub fn is_plugin_path(path: &[u8]) -> bool {
    path.len() > 1
        && path.first() == Some(&b'/')
        && !path.windows(2).any(|w| w == b"..")
        && !is_sensitive(path)
}

/// Turns the first `len` bytes of `path` into the absolute path a request names: [normalised]
/// (normalise), with a leading `/` and no trailing one (`/` alone for the root). Needs one byte
/// of room after `len`; returns the new length.
pub fn web_path(path: &mut [u8], len: usize) -> usize {
    let Some(text) = path.get_mut(..len) else {
        return 0;
    };
    let kept = normalise(text);
    let Some(out) = path.get_mut(..=kept) else {
        return 0;
    };
    out.rotate_right(1);
    if let Some(slash) = out.first_mut() {
        *slash = b'/';
    }
    kept + 1
}

/// Where the path starts in a request URL (`http://host/path`), or `None` when the URL names a
/// host but no path. A value without `://` is a path already.
#[must_use]
pub fn url_path_start(url: &[u8]) -> Option<usize> {
    let Some(scheme) = url.windows(3).position(|w| w == b"://") else {
        return Some(0);
    };
    let host = scheme + 3;
    let slash = url.get(host..)?.iter().position(|&b| b == b'/')?;
    Some(host + slash)
}

/// The byte `b` of a path needs a `%XX` escape in a URL: a space, `%`, `#`, `?`, `&` or a
/// non-ASCII byte.
fn needs_escape(b: u8) -> bool {
    matches!(b, b' ' | b'%' | b'#' | b'?' | b'&') || b > 127
}

/// Writes `path` into `out` as a URL path, escaping the bytes [`needs_escape`] picks with
/// upper-case hex, and returns the length; `out` holds at least three times `path`.
pub fn encode_path(path: &[u8], out: &mut [u8]) -> usize {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut len = 0;
    let mut put = |b: u8| {
        if let Some(slot) = out.get_mut(len) {
            *slot = b;
            len += 1;
        }
    };
    for &b in path {
        if needs_escape(b) {
            put(b'%');
            put(HEX.get(usize::from(b >> 4)).copied().unwrap_or(b'0'));
            put(HEX.get(usize::from(b & 15)).copied().unwrap_or(b'0'));
        } else {
            put(b);
        }
    }
    len
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::String;
    use std::vec::Vec;

    fn web(s: &[u8]) -> Vec<u8> {
        let mut b = s.to_vec();
        b.push(0);
        let n = web_path(&mut b, s.len());
        b.truncate(n);
        b
    }

    fn encoded(s: &[u8]) -> Vec<u8> {
        let mut out = std::vec![0; s.len() * 3];
        let n = encode_path(s, &mut out);
        out.truncate(n);
        out
    }

    #[test]
    fn hidden_names_and_components() {
        assert!(is_hidden_name(b".fonts") && is_hidden_name(b"XTCache"));
        assert!(!is_hidden_name(b"xtcache") && !is_hidden_name(b"Books") && !is_hidden_name(b""));
        assert!(is_system_name(b"System Volume Information") && !is_system_name(b".fonts"));
        assert!(has_hidden_component(b"/Books/.cache/x") && has_hidden_component(b"XTCache/a"));
        assert!(!has_hidden_component(b"//Books/a.b/") && !has_hidden_component(b""));
        // A request argument's "%00" ends the name SdFat creates.
        assert!(is_system_name(b"XTCache\0x") && !is_system_name(b"\0XTCache"));
        assert!(has_hidden_component(
            b"/Books/System Volume Information\0a/b"
        ));
    }

    #[test]
    fn credential_stores_however_spelled() {
        for path in [
            &b"/.crosspoint/wifi.json"[..],
            b"/.CrossPoint/WIFI.json",
            b"/.crosspoint/wifi.json.tmp",
            b"//.crosspoint/./x/../opds.json",
            b".crosspoint/koreader.json",
            b"/ .crosspoint ./ wifi.json.",
            b"/CROSSP~1/wifi.json",
            b"/.crosspoint/WIFI~1.JSO",
            b"/a/b/../../.crosspoint/wifi.json/more",
            b"/.crosspoint/wifi.json\0/..",
        ] {
            assert!(is_sensitive(path), "{}", String::from_utf8_lossy(path));
        }
        for path in [
            &b"/.crosspoint/settings.json"[..],
            b"/Books/Photos/IMG~1.JPG",
            b"/Books/a~b.epub",
            b"/Books/wifi.json",
            b"/.crosspoint/wifi",
            b"/.crosspoint",
            b"/.crosspoint/wifi.json/../x",
            b"/.crosspoint/wifi.json/..",
            b"/A~B/x",
            b"/AB~1.ABCD/x",
            b"/.crosspoint/wi\0fi.json",
            b"",
        ] {
            assert!(!is_sensitive(path), "{}", String::from_utf8_lossy(path));
        }
    }

    #[test]
    fn plugin_paths() {
        assert!(is_plugin_path(b"/Books/new.epub"));
        assert!(is_plugin_path(b"/.crosspoint/month-wallpaper.json"));
        assert!(!is_plugin_path(b"/") && !is_plugin_path(b"relative/x"));
        assert!(!is_plugin_path(b"/a/..b") && !is_plugin_path(b"/.crosspoint/./koreader.json"));
    }

    #[test]
    fn request_paths() {
        assert_eq!(web(b""), b"/");
        assert_eq!(web(b"/"), b"/");
        assert_eq!(web(b"Books/"), b"/Books");
        assert_eq!(web(b"//a/./b/../c//"), b"/a/./c");
        assert_eq!(web(b"/../../etc"), b"/etc");
        assert_eq!(url_path_start(b"http://host:80/a/b"), Some(14));
        assert_eq!(url_path_start(b"http://host"), None);
        assert_eq!(url_path_start(b"/plain"), Some(0));
        assert_eq!(url_path_start(b"x://"), None);
    }

    #[test]
    fn encodes_paths() {
        assert_eq!(encoded(b"/a b/%#?&=+"), b"/a%20b/%25%23%3F%26=+");
        assert_eq!(encoded("/ç".as_bytes()), b"/%C3%A7");
        assert_eq!(encoded(b""), b"");
        assert_eq!(encoded(b"\x7f\x80"), b"\x7f%80");
    }

    /// Random bytes, weighted toward the ones the rules look at: nothing panics, and the results
    /// keep their shape.
    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::indexing_slicing)]
    fn mini_fuzz() {
        const BYTES: &[u8] = b"/./~1.x%: \0\x7f\xc3wifi.json";
        let mut seed = 0x9E37_79B9u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..20_000 {
            let len = next() as usize % 40;
            let path: Vec<u8> = (0..len)
                .map(|_| match next() % 4 {
                    0 => next() as u8,
                    _ => BYTES[next() as usize % BYTES.len()],
                })
                .collect();
            let _ = (is_sensitive(&path), is_plugin_path(&path));
            let _ = (is_hidden_name(&path), has_hidden_component(&path));
            let made = web(&path);
            assert_eq!(made.first(), Some(&b'/'));
            assert!(made.len() == 1 || made.last() != Some(&b'/'));
            assert!(!made.windows(2).any(|w| w == b"//"));
            if let Some(start) = url_path_start(&path) {
                assert!(start <= path.len());
            }
            let out = encoded(&path);
            assert!(out.len() >= path.len() && out.len() <= path.len() * 3);
            assert!(out.iter().all(|&b| !needs_escape(b) || b == b'%'));
        }
    }
}
