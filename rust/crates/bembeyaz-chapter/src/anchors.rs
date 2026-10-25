//! Which page each of the chapter's element ids lands on, for links and the table of contents.
//! An id waits until the block before it is on a page: the page it lands on is the one its own
//! content starts. An id that starts a table-of-contents chapter starts a page of its own.

use alloc::vec::Vec;
use bembeyaz_layout::pager::{Pager, Pages};

/// Ids beyond this many are not recorded, except table-of-contents ones: converters that put an
/// id on every span would otherwise fill the heap.
pub const MAX_ANCHORS: usize = 1024;

/// A name in [`Anchors`]' buffer.
#[derive(Clone, Copy, Debug)]
struct Name {
    at: u32,
    len: u32,
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    name: Name,
    page: u16,
}

/// The chapter's anchors.
#[derive(Default)]
pub struct Anchors {
    /// Names back to back, each followed by a NUL; the pending one is last.
    names: Vec<u8>,
    table: Vec<Anchor>,
    pending: Option<Name>,
    /// The table of contents' ids, the same way.
    toc_names: Vec<u8>,
    toc: Vec<Name>,
}

/// Out of memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

fn push_name(buffer: &mut Vec<u8>, name: &[u8]) -> Result<Name, Full> {
    let len = u32::try_from(name.len()).map_err(|_| Full)?;
    let at = u32::try_from(buffer.len()).map_err(|_| Full)?;
    buffer.try_reserve(name.len() + 1).map_err(|_| Full)?;
    buffer.extend_from_slice(name);
    buffer.push(0);
    Ok(Name { at, len })
}

/// The name, followed by its NUL in the buffer (an empty one before a static NUL otherwise).
fn name_in(buffer: &[u8], name: Name) -> &[u8] {
    let at = name.at as usize;
    buffer
        .get(at..at + name.len as usize)
        .unwrap_or(&b"\0"[..0])
}

impl Anchors {
    /// No anchors and no table-of-contents ids.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an id the table of contents names.
    ///
    /// # Errors
    /// [`Full`] when it cannot be stored.
    pub fn add_toc(&mut self, name: &[u8]) -> Result<(), Full> {
        let stored = push_name(&mut self.toc_names, name)?;
        self.toc.try_reserve(1).map_err(|_| Full)?;
        self.toc.push(stored);
        Ok(())
    }

    fn is_toc(&self, name: &[u8]) -> bool {
        self.toc
            .iter()
            .any(|&n| name_in(&self.toc_names, n) == name)
    }

    /// An element with id `name` opened. Ids on elements that cannot be link targets (a
    /// `<span>`, not `navigable`) and ids past [`MAX_ANCHORS`] are dropped, unless the table of
    /// contents names them. One still waiting is placed first, at the page it waited on; an empty
    /// id then waits for nothing.
    ///
    /// # Errors
    /// [`Full`] when it cannot be stored.
    pub fn open(
        &mut self,
        cursor: &mut Pager,
        pages: &mut impl Pages,
        name: &[u8],
        navigable: bool,
    ) -> Result<(), Full> {
        if !self.is_toc(name) && (!navigable || self.table.len() >= MAX_ANCHORS) {
            return Ok(());
        }
        self.place_pending(cursor, pages)?;
        if !name.is_empty() {
            self.pending = Some(push_name(&mut self.names, name)?);
        }
        Ok(())
    }

    /// The block before the waiting id is on a page: the id lands on the page now current, a new
    /// one if the table of contents starts a chapter there.
    ///
    /// # Errors
    /// [`Full`] when it cannot be stored.
    pub fn place_pending(
        &mut self,
        cursor: &mut Pager,
        pages: &mut impl Pages,
    ) -> Result<(), Full> {
        let Some(name) = self.pending else {
            return Ok(());
        };
        if self.is_toc(name_in(&self.names, name)) && cursor.has_content() {
            cursor.complete_page(pages);
        }
        self.record_pending(cursor.completed())
    }

    /// The waiting id, if any, lands on `page` (a rule or the chapter's end placed it).
    ///
    /// # Errors
    /// [`Full`] when it cannot be stored.
    pub fn record_pending(&mut self, page: u16) -> Result<(), Full> {
        let Some(name) = self.pending.take() else {
            return Ok(());
        };
        self.table.try_reserve(1).map_err(|_| Full)?;
        self.table.push(Anchor { name, page });
        Ok(())
    }

    /// Whether an id is waiting.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The anchors placed, in order.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Whether none is placed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// The `i`th anchor placed: its name, followed by a NUL in the buffer, and its page.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<(&[u8], u16)> {
        self.table
            .get(i)
            .map(|a| (name_in(&self.names, a.name), a.page))
    }

    /// The page of the first anchor placed named `name`.
    #[must_use]
    pub fn find(&self, name: &[u8]) -> Option<u16> {
        self.table
            .iter()
            .find(|a| name_in(&self.names, a.name) == name)
            .map(|a| a.page)
    }
}
