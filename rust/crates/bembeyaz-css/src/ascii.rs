//! Bytes as CSS sees them: keywords, properties and selectors are ASCII and compare without
//! case; anything else passes through.

/// Whitespace between CSS tokens.
pub(crate) const fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0C)
}

pub(crate) fn trim_end(mut s: &[u8]) -> &[u8] {
    while let [rest @ .., last] = s
        && is_space(*last)
    {
        s = rest;
    }
    s
}

pub(crate) fn trim(mut s: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = s
        && is_space(*first)
    {
        s = rest;
    }
    trim_end(s)
}

/// `s` is `keyword` (lowercase) in any case.
pub(crate) fn is(s: &[u8], keyword: &str) -> bool {
    s.eq_ignore_ascii_case(keyword.as_bytes())
}

/// `s` ends with `keyword` (lowercase) in any case; the rest, if so.
pub(crate) fn strip_suffix<'a>(s: &'a [u8], keyword: &str) -> Option<&'a [u8]> {
    let at = s.len().checked_sub(keyword.len())?;
    let (rest, tail) = s.split_at_checked(at)?;
    is(tail, keyword).then_some(rest)
}

/// `s` starts with `keyword` (lowercase) in any case; the rest, if so.
pub(crate) fn strip_prefix<'a>(s: &'a [u8], keyword: &str) -> Option<&'a [u8]> {
    let (head, rest) = s.split_at_checked(keyword.len())?;
    is(head, keyword).then_some(rest)
}

/// The non-empty, trimmed runs between bytes that `delimiter` picks.
pub(crate) fn tokens(s: &[u8], delimiter: impl Fn(u8) -> bool) -> impl Iterator<Item = &[u8]> {
    s.split(move |&b| delimiter(b))
        .map(trim)
        .filter(|t| !t.is_empty())
}

/// The whitespace-separated words of `s`.
pub(crate) fn words(s: &[u8]) -> impl Iterator<Item = &[u8]> {
    tokens(s, is_space)
}

/// FNV-1a over the lowercase bytes. Never 0, so 0 can mean "absent".
pub(crate) fn hash(s: &[u8]) -> u32 {
    let hash = s.iter().fold(2_166_136_261u32, |hash, &b| {
        (hash ^ u32::from(b.to_ascii_lowercase())).wrapping_mul(16_777_619)
    });
    if hash == 0 { 1 } else { hash }
}
