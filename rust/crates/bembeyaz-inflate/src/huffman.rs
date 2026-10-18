//! Canonical Huffman codes (RFC 1951 §3.2.2): a direct lookup table for the short codes, which
//! are nearly all of them, and a count-by-length walk for the rest.

/// Result of looking a code up in the bits at hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lookup {
    /// The symbol and the length of its code.
    Found(u16, u32),
    /// The code continues past the bits at hand.
    NeedBits,
    /// No code starts with these bits.
    Invalid,
}

/// A code for `N` symbols with a lookup table of `T` entries (a power of two): codes no longer
/// than `log2(T)` bits resolve in one step. All-zero bytes are a valid, empty code.
pub(crate) struct Huffman<const N: usize, const T: usize> {
    /// Indexed by the next bits, least significant first: `symbol | length << 9`, or 0 when
    /// the code is longer than the table (or there is none).
    fast: [u16; T],
    /// Codes of each length, 1-15.
    count: [u16; 16],
    /// Symbols in canonical order: by code length, then by value.
    symbol: [u16; N],
    max_len: u32,
    /// The count-by-length walk after the table's length (`first`, `index`), so a longer code
    /// resumes there instead of starting over.
    walk_first: u32,
    walk_index: u32,
}

const MAX_BITS: usize = 15;

/// The low `len` bits of `code`, in reverse order.
fn reverse(code: u32, len: u32) -> u32 {
    code.reverse_bits() >> (32 - len)
}

impl<const N: usize, const T: usize> Huffman<N, T> {
    const FAST_BITS: u32 = T.trailing_zeros();

    /// Builds the code from each symbol's code length (0 for an unused symbol). An
    /// over-subscribed set of lengths is an error; so is an incomplete one, unless
    /// `allow_one_bit` and no code is longer than one bit (no codes at all, or a single one).
    pub(crate) fn build(&mut self, lengths: &[u8], allow_one_bit: bool) -> Result<(), ()> {
        let lengths = lengths.get(..N).unwrap_or(lengths);
        self.count = [0; 16];
        for &len in lengths {
            *self.count.get_mut(usize::from(len)).ok_or(())? += 1;
        }
        self.count[0] = 0;

        let mut left = 1i32;
        self.max_len = 0;
        for (len, &count) in (0u32..).zip(self.count.iter()).skip(1) {
            left = (left << 1) - i32::from(count);
            if left < 0 {
                return Err(());
            }
            if count > 0 {
                self.max_len = len;
            }
        }
        if left > 0 && !(allow_one_bit && self.max_len <= 1) {
            return Err(());
        }

        // Where each length's symbols start in `symbol`.
        let mut offset = [0u16; MAX_BITS + 1];
        let mut sum = 0;
        for (start, &count) in offset.iter_mut().zip(self.count.iter()).skip(1) {
            *start = sum;
            sum += count;
        }
        for (sym, &len) in (0u16..).zip(lengths.iter()) {
            if len != 0
                && let Some(at) = offset.get_mut(usize::from(len))
            {
                if let Some(slot) = self.symbol.get_mut(usize::from(*at)) {
                    *slot = sym;
                }
                *at += 1;
            }
        }

        let (mut first, mut index) = (0u32, 0u32);
        for len in 1..=Self::FAST_BITS.min(15) {
            let count = u32::from(self.count.get(len as usize).copied().unwrap_or(0));
            index += count;
            first = (first + count) << 1;
        }
        self.walk_first = first;
        self.walk_index = index;

        self.fast.fill(0);
        let mut code = 0u32;
        let mut index = 0;
        for len in 1..=Self::FAST_BITS.min(15) {
            for _ in 0..self.count.get(len as usize).copied().unwrap_or(0) {
                let sym = self.symbol.get(index).copied().unwrap_or(0);
                index += 1;
                #[allow(clippy::cast_possible_truncation, reason = "len <= 15, sym < 512")]
                let entry = sym | (len as u16) << 9;
                let mut at = reverse(code, len) as usize;
                while let Some(slot) = self.fast.get_mut(at) {
                    *slot = entry;
                    at += 1 << len;
                }
                code += 1;
            }
            code <<= 1;
        }
        Ok(())
    }

    /// Looks up the code at the start of `bits`, of which `nbits` are valid.
    #[inline]
    pub(crate) fn lookup(&self, bits: u32, nbits: u32) -> Lookup {
        let entry = self
            .fast
            .get((bits & ((1 << Self::FAST_BITS) - 1)) as usize)
            .copied()
            .unwrap_or(0);
        if entry != 0 {
            let len = u32::from(entry >> 9);
            return if len <= nbits {
                Lookup::Found(entry & 0x1FF, len)
            } else {
                Lookup::NeedBits
            };
        }
        self.lookup_slow(bits, nbits)
    }

    /// Walks the codes length by length (puff's method), for those longer than the table.
    #[cold]
    fn lookup_slow(&self, bits: u32, nbits: u32) -> Lookup {
        // Even no code at all reads a bit first, as the old decoder did.
        if nbits == 0 {
            return Lookup::NeedBits;
        }
        let (mut code, mut first, mut index, mut len) = (0u32, 0u32, 0usize, 1);
        // Past the table's length the code is complete, and no table entry means a longer code:
        // the walk resumes after the table's bits, read most significant first.
        if self.max_len > Self::FAST_BITS {
            if nbits <= Self::FAST_BITS {
                return Lookup::NeedBits;
            }
            code = reverse(bits, Self::FAST_BITS) << 1;
            first = self.walk_first;
            index = self.walk_index as usize;
            len = Self::FAST_BITS + 1;
        }
        for len in len..=self.max_len {
            if len > nbits {
                return Lookup::NeedBits;
            }
            code |= (bits >> (len - 1)) & 1;
            let count = u32::from(self.count.get(len as usize).copied().unwrap_or(0));
            if code.wrapping_sub(first) < count {
                let at = index + (code - first) as usize;
                return self
                    .symbol
                    .get(at)
                    .map_or(Lookup::Invalid, |&sym| Lookup::Found(sym, len));
            }
            index += count as usize;
            first = (first + count) << 1;
            code <<= 1;
        }
        Lookup::Invalid
    }
}
