//! The few DER (ITU-T X.690) reads a certificate needs: one element at a time, definite
//! lengths only.

/// The tags read here.
pub(crate) const INTEGER: u8 = 0x02;
pub(crate) const BIT_STRING: u8 = 0x03;
pub(crate) const OID: u8 = 0x06;
pub(crate) const UTC_TIME: u8 = 0x17;
pub(crate) const GENERALIZED_TIME: u8 = 0x18;
pub(crate) const SEQUENCE: u8 = 0x30;
/// A certificate's `[0] EXPLICIT` version.
pub(crate) const VERSION: u8 = 0xA0;

/// One element: its tag, its content, and the whole of it (tag and length too).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Element<'a> {
    pub tag: u8,
    pub content: &'a [u8],
    pub whole: &'a [u8],
}

/// The element `input` starts with, and what follows it; none when it is not DER this reads (a
/// tag of more than one byte, an indefinite or overlong length, or one past the end).
pub(crate) fn next(input: &[u8]) -> Option<(Element<'_>, &[u8])> {
    let (&tag, rest) = input.split_first()?;
    if tag & 0x1F == 0x1F {
        return None;
    }
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        long_length(first & 0x7F, rest)?
    };
    let header = input.len() - rest.len();
    let content = rest.get(..len)?;
    let whole = input.get(..header + len)?;
    let after = rest.get(len..)?;
    Some((
        Element {
            tag,
            content,
            whole,
        },
        after,
    ))
}

/// A long-form length of `count` bytes (1 to 4, no leading zero, not below 128).
fn long_length(count: u8, rest: &[u8]) -> Option<(usize, &[u8])> {
    if !(1..=4).contains(&count) {
        return None;
    }
    let (bytes, rest) = rest.split_at_checked(usize::from(count))?;
    if bytes.first() == Some(&0) {
        return None;
    }
    let len = bytes
        .iter()
        .fold(0usize, |len, b| len << 8 | usize::from(*b));
    (len >= 0x80).then_some((len, rest))
}

/// The element `input` starts with, if its tag is `tag`.
pub(crate) fn expect(input: &[u8], tag: u8) -> Option<(Element<'_>, &[u8])> {
    next(input).filter(|(e, _)| e.tag == tag)
}
