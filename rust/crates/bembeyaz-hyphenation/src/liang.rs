//! Liang's patterns, from the binary tries of typst/hypher.
//!
//! A trie file starts with the root node's address (4 bytes, big-endian), then the levels
//! tape, then the nodes. A node is a header byte (bit 7: has levels; bits 6–5: the byte width
//! of child offsets, 0 meaning 1; bits 4–0: the child count, 31 meaning a count byte follows),
//! for levels a 12-bit tape address and a 4-bit length, then the children's bytes and their
//! signed offsets from this node. A level byte is `distance * 10 + level`, the distance in
//! bytes from the previous one (the first from where the match started).
//!
//! Node addresses count from the byte after the root address, as the C++ code they replace did;
//! tape addresses count from the start of the file.

/// Longest word looked up, in letters plus the two dots around it, and its bytes.
const MAX_CHARS: usize = 70;
const MAX_BYTES: usize = 160;

/// Words of more letters get no pattern breaks.
pub const MAX_LETTERS: usize = MAX_CHARS - 2;

pub struct Trie {
    /// The file without its root address.
    nodes: &'static [u8],
    /// The whole file, for the levels tape.
    file: &'static [u8],
    root: usize,
}

impl Trie {
    pub const fn new(file: &'static [u8]) -> Self {
        let (root, nodes) = match file {
            [a, b, c, d, nodes @ ..] => (
                (u32::from_be_bytes([*a, *b, *c, *d]) as usize).wrapping_sub(4),
                nodes,
            ),
            _ => (usize::MAX, &[] as &[u8]),
        };
        Trie { nodes, file, root }
    }

    fn node(&self, addr: usize) -> Option<Node> {
        let mut at = addr;
        let mut take = |n: usize| {
            let bytes = self.nodes.get(at..at.checked_add(n)?)?;
            at += n;
            Some(bytes)
        };
        let &[header] = take(1)? else { return None };
        let stride = match (header >> 5) & 3 {
            0 => 1,
            s => usize::from(s),
        };
        let mut children = usize::from(header & 0x1F);
        if children == 31 {
            let &[count] = take(1)? else { return None };
            children = usize::from(count);
        }
        let mut levels: &[u8] = &[];
        if header & 0x80 != 0 {
            let &[hi, lo] = take(2)? else { return None };
            let start = usize::from(hi) << 4 | usize::from(lo >> 4);
            let len = usize::from(lo & 0x0F);
            if start + len > self.nodes.len() {
                return None;
            }
            levels = self.file.get(start..start + len)?;
        }
        Some(Node {
            addr,
            stride,
            bytes: take(children)?,
            targets: take(children * stride)?,
            levels,
        })
    }

    /// The child of `node` along `byte`.
    fn next(&self, node: &Node, byte: u8) -> Option<Node> {
        let i = node.bytes.iter().position(|&b| b == byte)?;
        let delta = node.targets.get(i * node.stride..(i + 1) * node.stride)?;
        let delta = match *delta {
            [a] => i64::from(i8::from_be_bytes([a])),
            [a, b] => i64::from(i16::from_be_bytes([a, b])),
            [a, b, c] => i64::from(u32::from_be_bytes([0, a, b, c])) - (1 << 23),
            _ => return None,
        };
        let addr = usize::try_from(i64::try_from(node.addr).ok()? + delta).ok()?;
        if addr >= self.nodes.len() {
            return None;
        }
        self.node(addr)
    }
}

struct Node {
    addr: usize,
    stride: usize,
    bytes: &'static [u8],
    targets: &'static [u8],
    levels: &'static [u8],
}

/// The letters a language's patterns cover, and how they lowercase.
pub struct Script {
    pub is_letter: fn(u32) -> bool,
    pub lower: fn(u32) -> u32,
}

/// The pattern breaks of a word of `script`'s letters:
/// bit `i` is a break before letter `i`, at least `min_prefix` letters from the start and
/// `min_suffix` from the end. 0 for a word of other characters or more than
/// [`MAX_LETTERS`] letters.
pub fn breaks(
    trie: &Trie,
    letters: &[u32],
    script: &Script,
    min_prefix: usize,
    min_suffix: usize,
) -> u128 {
    let mut word = Dotted::new();
    if word.fill(letters, script).is_none() {
        return 0;
    }
    let Some(root) = trie.node(trie.root) else {
        return 0;
    };
    let chars = word.chars;
    // One score per dotted character; odd scores are breaks.
    let mut scores = [0u8; MAX_CHARS];
    for start in (0..word.len).filter(|&b| word.char_at.get(b).is_some_and(|&c| c != NOT_A_START)) {
        let mut node = None::<Node>;
        for &byte in word.bytes.get(start..word.len).unwrap_or_default() {
            let Some(next) = trie.next(node.as_ref().unwrap_or(&root), byte) else {
                break;
            };
            let mut offset = start;
            for &packed in next.levels {
                offset += usize::from(packed / 10);
                // Only a split before a letter, past the first and before the last.
                let Some(&char) = word.char_at.get(offset).filter(|&&c| c != NOT_A_START) else {
                    continue;
                };
                let char = usize::from(char);
                if char < 2 || char + 2 > chars {
                    continue;
                }
                if let Some(score) = scores.get_mut(char) {
                    *score = (*score).max(packed % 10);
                }
            }
            node = Some(next);
        }
    }

    let count = letters.len();
    let mut out = 0u128;
    for i in min_prefix.max(1)..count {
        if count - i >= min_suffix && scores.get(i + 1).is_some_and(|s| s & 1 == 1) {
            out |= 1 << i;
        }
    }
    out
}

const NOT_A_START: u8 = u8::MAX;

/// The word between dots, lowercase UTF-8, with where each character starts.
struct Dotted {
    bytes: [u8; MAX_BYTES],
    len: usize,
    chars: usize,
    /// For each byte, the character starting there, or [`NOT_A_START`].
    char_at: [u8; MAX_BYTES],
}

impl Dotted {
    const fn new() -> Self {
        Dotted {
            bytes: [0; MAX_BYTES],
            len: 0,
            chars: 0,
            char_at: [NOT_A_START; MAX_BYTES],
        }
    }

    /// Writes `letters` between dots; `None` for an empty or too long word, or one of other
    /// characters.
    fn fill(&mut self, letters: &[u32], script: &Script) -> Option<()> {
        if letters.is_empty() || letters.len() > MAX_LETTERS {
            return None;
        }
        self.push(b".")?;
        for &cp in letters {
            if !(script.is_letter)(cp) {
                return None;
            }
            let mut buf = [0; 4];
            let encoded = bembeyaz_utf8::encode((script.lower)(cp), &mut buf);
            self.push(encoded)?;
        }
        self.push(b".")
    }

    /// Appends one character's bytes.
    fn push(&mut self, bytes: &[u8]) -> Option<()> {
        let end = self.len + bytes.len();
        self.bytes.get_mut(self.len..end)?.copy_from_slice(bytes);
        if self.chars >= MAX_CHARS {
            return None;
        }
        *self.char_at.get_mut(self.len)? = u8::try_from(self.chars).ok()?;
        self.len = end;
        self.chars += 1;
        Some(())
    }
}
