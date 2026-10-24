//! A `StarDict` HTML definition (tag soup) as the well-formed XHTML document the chapter parser lays
//! out (src/util/DictHtmlPages): wrapped in `<html><body>`, tag names lower-cased (and cut at 15
//! characters), void elements self-closed (`<br>` to `<br/>`) and their closers (`</br>`) dropped,
//! comments, doctypes and processing instructions dropped, and `<` and `&` outside markup or an
//! entity reference escaped. Attributes are copied as they are; damage this cannot repair
//! (mismatched tags, unquoted values) is left for the parser to reject.

/// The elements HTML leaves open, which XML needs closed.
const VOID: [&[u8]; 14] = [
    b"area", b"base", b"br", b"col", b"embed", b"hr", b"img", b"input", b"link", b"meta", b"param",
    b"source", b"track", b"wbr",
];

/// Tag names keep at most this many characters.
const NAME_MAX: usize = 15;

/// Where the entity reference (`&name;`, `&#123;` or `&#x1F;`) starting `html` ends: its `;`.
fn entity_end(html: &[u8]) -> Option<usize> {
    let rest = html.get(1..)?;
    let (start, digit): (usize, fn(&u8) -> bool) = match rest {
        [b'#', b'x' | b'X', ..] => (3, u8::is_ascii_hexdigit),
        [b'#', ..] => (2, u8::is_ascii_hexdigit),
        _ => (1, u8::is_ascii_alphanumeric),
    };
    let run = html.get(start..)?.iter().take_while(|b| digit(b)).count();
    let end = start + run;
    (run > 0 && html.get(end) == Some(&b';')).then_some(end)
}

/// Where the tag starting `html` (at its `<`) ends: its `>`, past quoted attribute values.
fn tag_end(html: &[u8]) -> Option<usize> {
    let mut quote = None;
    for (j, &d) in html.iter().enumerate().skip(1) {
        match quote {
            Some(q) if d == q => quote = None,
            None if d == b'"' || d == b'\'' => quote = Some(d),
            None if d == b'>' => return Some(j),
            _ => {}
        }
    }
    None
}

/// Writes `html` as XHTML to `out` in pieces; false as soon as `out` is.
pub fn normalize(html: &[u8], mut out: impl FnMut(&[u8]) -> bool) -> bool {
    if !out(b"<html><body>") {
        return false;
    }
    let n = html.len();
    let mut i = 0;
    // The start of text not written yet, written whole at the next markup.
    let mut text = 0;
    while i < n {
        let rest = html.get(i..).unwrap_or_default();
        let piece: &[u8];
        let next;
        match rest {
            [b'<', b'!' | b'?', ..] => {
                // A comment, doctype or processing instruction: dropped whole.
                next = if rest.starts_with(b"<!--") {
                    find(rest.get(4..).unwrap_or_default(), b"-->").map_or(n, |j| i + 4 + j + 3)
                } else {
                    rest.iter()
                        .position(|&b| b == b'>')
                        .map_or(n, |j| i + j + 1)
                };
                piece = b"";
            }
            [b'<', b, ..] if *b == b'/' || b.is_ascii_alphabetic() => {
                if !(flush(html, text, i, &mut out)) {
                    return false;
                }
                let Some(j) = tag_end(rest) else {
                    // Never closed: the '<' is text.
                    if !out(b"&lt;") {
                        return false;
                    }
                    i += 1;
                    text = i;
                    continue;
                };
                if !tag(rest.get(..=j).unwrap_or_default(), &mut out) {
                    return false;
                }
                i += j + 1;
                text = i;
                continue;
            }
            [b'<', ..] => {
                piece = b"&lt;";
                next = i + 1;
            }
            [b'&', ..] => {
                if let Some(end) = entity_end(rest) {
                    i += end + 1;
                    continue;
                }
                piece = b"&amp;";
                next = i + 1;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        if !(flush(html, text, i, &mut out) && (piece.is_empty() || out(piece))) {
            return false;
        }
        i = next;
        text = i;
    }
    flush(html, text, n, &mut out) && out(b"</body></html>")
}

/// Writes the text from `from` up to `to` as it is.
fn flush(html: &[u8], from: usize, to: usize, out: &mut impl FnMut(&[u8]) -> bool) -> bool {
    match html.get(from..to) {
        Some(text) if !text.is_empty() => out(text),
        _ => true,
    }
}

/// Where `needle` first starts in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Writes the tag `tag` (from its `<` to its `>`) with its name lower-cased, a void element
/// self-closed, or nothing for a void element's closer.
fn tag(tag: &[u8], out: &mut impl FnMut(&[u8]) -> bool) -> bool {
    let closing = tag.get(1) == Some(&b'/');
    let name_start = if closing { 2 } else { 1 };
    let inner = tag.get(name_start..tag.len() - 1).unwrap_or_default();
    let name_len = inner
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    let mut name = [0u8; NAME_MAX];
    let kept = name_len.min(NAME_MAX);
    for (d, s) in name.iter_mut().zip(inner).take(kept) {
        *d = s.to_ascii_lowercase();
    }
    let name = name.get(..kept).unwrap_or_default();
    let void = VOID.contains(&name);
    if closing && void {
        return true;
    }
    let attributes = inner.get(name_len..).unwrap_or_default();
    // The byte before the '>'.
    let self_closed = tag.len() >= 2 && tag.get(tag.len() - 2) == Some(&b'/');
    out(if closing { b"</" } else { b"<" })
        && out(name)
        && out(attributes)
        && (closing || !void || self_closed || out(b"/"))
        && out(b">")
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::normalize;
    use std::vec::Vec;

    fn xhtml(html: &str) -> std::string::String {
        let mut out = Vec::new();
        assert!(normalize(html.as_bytes(), |piece| {
            out.extend_from_slice(piece);
            true
        }));
        std::string::String::from_utf8(out).unwrap_or_default()
    }

    #[test]
    fn tag_soup_becomes_xhtml() {
        assert_eq!(
            xhtml("<B>Tom & Jerry</B><BR><img src='a>b'></br>x < y &amp; &#x1F; &#;"),
            "<html><body><b>Tom &amp; Jerry</b><br/><img src='a>b'/>x &lt; y &amp; &#x1F; &amp;#;</body></html>"
        );
        assert_eq!(
            xhtml("<!-- c --><!DOCTYPE x><?pi?>a<br/><a href=\"x\""),
            "<html><body>a<br/>&lt;a href=\"x\"</body></html>"
        );
        assert_eq!(
            xhtml("<ABCDEFGHIJKLMNOPQ x>"),
            "<html><body><abcdefghijklmno x></body></html>"
        );
        assert_eq!(xhtml("<!-- open"), "<html><body></body></html>");
    }

    #[test]
    fn a_failed_write_stops_it() {
        let mut calls = 0;
        assert!(!normalize(b"a<b>c", |_| {
            calls += 1;
            calls < 3
        }));
        assert_eq!(calls, 3);
    }
}
