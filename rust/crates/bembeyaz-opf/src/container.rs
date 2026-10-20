//! `META-INF/container.xml`: where the package document is, as lib/Epub's `ContainerParser` read
//! it up to ec0feded, which `test/container_parser` pins. The path is the `full-path` of a
//! `rootfile` with the package media type inside `container`/`rootfiles`, as its attribute value
//! (entities decoded, not resolved against any directory); with several, the last one wins. Names
//! are matched whole (no namespace processing). After `</container>` a later `container` is
//! searched too; the path found so far is kept.

use alloc::vec::Vec;
use bembeyaz_xml::{Error, Flow, Handler, Parser, StartTag, Str};

const PACKAGE_TYPE: &[u8] = b"application/oebps-package+xml";

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Start,
    InContainer,
    InRootfiles,
}

/// A container document being read.
pub struct Container {
    parser: Parser,
    reader: Reader,
}

struct Reader {
    state: State,
    full_path: Vec<u8>,
    /// The path changed since `take_changed` last said so.
    changed: bool,
    out_of_memory: bool,
}

impl Container {
    /// `limit` bounds the XML parser (see `Parser::new`).
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Container {
            parser: Parser::new(limit),
            reader: Reader {
                state: State::Start,
                full_path: Vec::new(),
                changed: false,
                out_of_memory: false,
            },
        }
    }

    /// Reads the next piece of the document; `last` when nothing follows.
    ///
    /// # Errors
    ///
    /// The parser's error; `Error::NoMemory` also when the path could not be held.
    pub fn feed(&mut self, data: &[u8], last: bool) -> Result<(), Error> {
        match self.parser.feed(data, last, &mut self.reader) {
            Err(Error::Aborted) if self.reader.out_of_memory => Err(Error::NoMemory),
            result => result,
        }
    }

    /// The package document's path read so far; empty when none.
    #[must_use]
    pub fn full_path(&self) -> &[u8] {
        &self.reader.full_path
    }

    /// Whether the path changed since this was last asked, so a caller that copies it copies it
    /// once per change rather than after every piece.
    pub fn take_changed(&mut self) -> bool {
        core::mem::take(&mut self.reader.changed)
    }

    /// Where the token that failed starts in the input, in bytes.
    #[must_use]
    pub fn error_offset(&self) -> u64 {
        self.parser.error_offset()
    }
}

impl Handler for Reader {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        match (self.state, tag.name().bytes()) {
            (State::Start, b"container") => self.state = State::InContainer,
            (State::InContainer, b"rootfiles") => self.state = State::InRootfiles,
            (State::InRootfiles, b"rootfile") => {
                let mut media_type: Option<Str<'_>> = None;
                let mut path: Option<Str<'_>> = None;
                for (name, value) in tag.attributes().iter() {
                    match name.bytes() {
                        b"media-type" => media_type = Some(value),
                        b"full-path" => path = Some(value),
                        _ => {}
                    }
                }
                if let (Some(media_type), Some(path)) = (media_type, path)
                    && media_type.bytes() == PACKAGE_TYPE
                {
                    self.full_path.clear();
                    self.changed = true;
                    if self.full_path.try_reserve(path.bytes().len()).is_err() {
                        self.out_of_memory = true;
                        return Flow::Break(());
                    }
                    self.full_path.extend_from_slice(path.bytes());
                    self.changed = true;
                }
            }
            _ => {}
        }
        Flow::Continue(())
    }

    fn end_element(&mut self, name: Str<'_>) -> Flow {
        match (self.state, name.bytes()) {
            (State::InRootfiles, b"rootfiles") => self.state = State::InContainer,
            (State::InContainer, b"container") => self.state = State::Start,
            _ => {}
        }
        Flow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]

    use super::Container;
    use std::vec::Vec;

    fn read(doc: &[u8], chunk: usize) -> (Vec<u8>, bool) {
        let mut container = Container::new(1 << 20);
        let pieces: Vec<&[u8]> = doc.chunks(chunk.max(1)).collect();
        let mut ok = true;
        for (i, piece) in pieces.iter().enumerate() {
            ok &= container.feed(piece, i + 1 == pieces.len()).is_ok();
        }
        (container.full_path().to_vec(), ok)
    }

    const DOC: &[u8] = br#"<?xml version="1.0"?><container><rootfiles>
<rootfile full-path="first.opf" media-type="application/oebps-package+xml"/>
<rootfile full-path="x.pdf" media-type="application/pdf"/>
<rootfile media-type="application/oebps-package+xml" full-path="OEBPS/c&amp;d.opf"/>
</rootfiles></container>"#;

    #[test]
    fn the_last_package_rootfile_wins_in_any_pieces() {
        for chunk in [1, 3, 7, DOC.len()] {
            assert_eq!(
                read(DOC, chunk),
                (b"OEBPS/c&d.opf".to_vec(), true),
                "{chunk}"
            );
        }
    }

    #[test]
    fn take_changed_reports_a_new_path_once() {
        let mut container = Container::new(1 << 20);
        assert!(!container.take_changed());
        assert!(container.feed(DOC, false).is_ok());
        assert!(container.take_changed());
        assert!(!container.take_changed());
        // Nothing new: the path stays and is not reported again.
        assert!(container.feed(b"", true).is_ok());
        assert!(!container.take_changed());
        assert_eq!(container.full_path(), b"OEBPS/c&d.opf");
    }

    #[test]
    fn truncated_and_corrupted_input_never_panics() {
        let mut seed = 0x9E37_79B9_u32;
        for end in 0..=DOC.len() {
            let _ = read(&DOC[..end], 4);
        }
        for i in 0..DOC.len() {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut copy = DOC.to_vec();
            copy[i] = seed.to_le_bytes()[0];
            let _ = read(&copy, 1 + (seed as usize % 7));
        }
    }
}
