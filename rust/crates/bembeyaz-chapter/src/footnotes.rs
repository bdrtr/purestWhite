//! Note references (`<a epub:type="noteref" href="#n1">[1]</a>`) waiting for their page: a
//! reference's number is collected while its link is open, and it goes to the page that shows the
//! word it follows.

use alloc::vec::Vec;

/// A note's number keeps at most this many bytes, its target fewer than [`HREF_LEN`]; one more
/// for the NUL in each case (the C++ `FootnoteEntry`'s arrays).
pub const NUMBER_LEN: usize = 32;
pub const HREF_LEN: usize = 256;

/// Out of memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

#[derive(Clone, Copy, Debug)]
struct Pending {
    /// The words of its block laid out before it is shown.
    word: i32,
    /// Its number and target in the buffer, each followed by a NUL.
    at: u32,
}

/// The open reference and the ones waiting.
#[derive(Default)]
pub struct Footnotes {
    /// The open link's target, followed by a NUL; empty when it has none.
    href: Vec<u8>,
    number: [u8; NUMBER_LEN],
    number_len: u8,
    /// Numbers and targets back to back, each followed by a NUL.
    buffer: Vec<u8>,
    pending: Vec<Pending>,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\r' | b'\n' | b'\t')
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or_default()
}

impl Footnotes {
    /// No reference open or waiting.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A reference opens with target `href`: empty, or one the block keeps (fewer than
    /// [`HREF_LEN`] bytes).
    ///
    /// # Errors
    /// [`Full`] when the target cannot be stored; the link then has none.
    pub fn open(&mut self, href: &[u8]) -> Result<(), Full> {
        self.href.clear();
        self.number_len = 0;
        let href = until_nul(href);
        if href.is_empty() {
            return Ok(());
        }
        let href = href.get(..HREF_LEN - 1).unwrap_or(href);
        self.href.try_reserve(href.len() + 1).map_err(|_| Full)?;
        self.href.extend_from_slice(href);
        self.href.push(0);
        Ok(())
    }

    /// The open reference's target followed by a NUL, or just a NUL.
    #[must_use]
    pub fn href(&self) -> &[u8] {
        if self.href.is_empty() {
            b"\0"
        } else {
            &self.href
        }
    }

    /// Text inside the open reference: without the spaces and square brackets around it, as much
    /// as still fits the number (`" [ 12 ] "` is `12`).
    pub fn text(&mut self, text: &[u8]) {
        let start = text
            .iter()
            .position(|&b| !(is_space(b) || b == b'['))
            .unwrap_or(text.len());
        let text = text.get(start..).unwrap_or_default();
        let end = text
            .iter()
            .rposition(|&b| !(is_space(b) || b == b']'))
            .map_or(0, |at| at + 1);
        let room = NUMBER_LEN - 1 - usize::from(self.number_len);
        for &b in text.iter().take(end).take(room) {
            if let Some(slot) = self.number.get_mut(usize::from(self.number_len)) {
                *slot = b;
                self.number_len += 1;
            }
        }
    }

    /// The open reference closes after the block's first `word` words: it waits for its page if
    /// it has a number and a target.
    ///
    /// # Errors
    /// [`Full`] when it cannot be stored.
    pub fn close(&mut self, word: i32) -> Result<(), Full> {
        let number = until_nul(
            self.number
                .get(..usize::from(self.number_len))
                .unwrap_or_default(),
        );
        let href = until_nul(&self.href);
        if number.is_empty() || href.is_empty() {
            return Ok(());
        }
        let at = u32::try_from(self.buffer.len()).map_err(|_| Full)?;
        self.buffer
            .try_reserve(number.len() + href.len() + 2)
            .map_err(|_| Full)?;
        self.pending.try_reserve(1).map_err(|_| Full)?;
        self.buffer.extend_from_slice(number);
        self.buffer.push(0);
        self.buffer.extend_from_slice(href);
        self.buffer.push(0);
        self.pending.push(Pending { word, at });
        Ok(())
    }

    /// Hands `page` (number and target, each followed by a NUL) the references waiting for no
    /// more than `words` of their block's words, in order.
    pub fn take(&mut self, words: i32, mut page: impl FnMut(&[u8], &[u8])) {
        let ready = self.pending.iter().take_while(|p| p.word <= words).count();
        if ready == 0 {
            return;
        }
        for p in self.pending.iter().take(ready) {
            let entry = self.buffer.get(p.at as usize..).unwrap_or(b"\0\0");
            let number_end = entry
                .iter()
                .position(|&b| b == 0)
                .map_or(entry.len(), |at| at + 1);
            let (number, rest) = entry.split_at(number_end);
            let href_end = rest
                .iter()
                .position(|&b| b == 0)
                .map_or(rest.len(), |at| at + 1);
            page(number, rest.get(..href_end).unwrap_or(b"\0"));
        }
        self.pending.drain(..ready);
        match self.pending.first() {
            None => self.buffer.clear(),
            Some(first) => {
                let used = first.at;
                self.buffer.drain(..used as usize);
                for p in &mut self.pending {
                    p.at -= used;
                }
            }
        }
    }

    /// Hands `page` every reference still waiting.
    pub fn take_all(&mut self, page: impl FnMut(&[u8], &[u8])) {
        self.take(i32::MAX, page);
    }

    /// Whether any reference waits.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}
