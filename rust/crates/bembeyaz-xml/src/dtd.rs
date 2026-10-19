//! The prolog: expat's xmlrole.c grammar (without parameter entity parsing, as the firmware builds
//! expat), and what the internal DTD subset declares that changes what content reports: general
//! entities and attribute defaults.

use alloc::vec::Vec;

use crate::Error;
use crate::scan::Prolog;

/// Where the prolog grammar is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    Prolog0,
    Prolog1,
    Prolog2,
    Doctype0,
    Doctype1,
    Doctype2,
    Doctype3,
    Doctype4,
    Doctype5,
    InternalSubset,
    Entity0,
    Entity1,
    Entity2,
    Entity3,
    Entity4,
    Entity5,
    Entity6,
    Entity7,
    Entity8,
    Entity9,
    Entity10,
    Notation0,
    Notation1,
    Notation2,
    Notation3,
    Notation4,
    Attlist0,
    Attlist1,
    Attlist2,
    Attlist3,
    Attlist4,
    Attlist5,
    Attlist6,
    Attlist7,
    Attlist8,
    Attlist9,
    Element0,
    Element1,
    Element2,
    Element3,
    Element4,
    Element5,
    Element6,
    Element7,
    DeclClose,
    Done,
}

/// What a prolog token means where it appears.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Role {
    None,
    XmlDecl,
    InstanceStart,
    DoctypePublicId,
    DoctypeSystemId,
    Pi,
    Comment,
    GeneralEntityName,
    ParamEntityName,
    EntityValue,
    EntityPublicId,
    EntityComplete,
    EntityNotationName,
    NotationPublicId,
    AttlistElementName,
    AttributeName,
    /// An attribute type: whether it is CDATA, and whether it is ID.
    AttributeType {
        cdata: bool,
        id: bool,
    },
    /// `#IMPLIED` or `#REQUIRED`.
    NoDefault,
    /// A default value, `#FIXED` or not.
    DefaultValue,
    GroupOpen,
    GroupSequence,
    GroupChoice,
    Error,
}

/// The token's text after its first `skip` bytes.
fn word(b: &[u8], start: usize, skip: usize, end: usize) -> &[u8] {
    b.get(start + skip..end).unwrap_or_default()
}

pub(crate) struct Grammar {
    pub(crate) state: State,
    /// How deep the content model group is.
    level: usize,
}

impl Grammar {
    pub(crate) const fn new() -> Self {
        Grammar {
            state: State::Prolog0,
            level: 0,
        }
    }

    /// The role of the token `tok` at `b[start..end]`, moving the grammar on. One arm per
    /// transition of xmlrole.c, so arms with the same result stay apart.
    #[allow(clippy::too_many_lines, clippy::match_same_arms)]
    pub(crate) fn role(&mut self, tok: Prolog, b: &[u8], start: usize, end: usize) -> Role {
        use Prolog as T;
        use State as S;
        let name = || word(b, start, 0, end);
        let decl = |name_end: usize| word(b, start, 2, name_end);
        let pound = || word(b, start, 1, end);
        let here = self.state;
        let (state, role) = match (self.state, tok) {
            (S::Prolog0, T::S) => (S::Prolog1, Role::None),
            (S::Prolog0, T::XmlDecl) => (S::Prolog1, Role::XmlDecl),
            (S::Prolog0 | S::Prolog1, T::Pi { .. }) => (S::Prolog1, Role::Pi),
            (S::Prolog0 | S::Prolog1, T::Comment) => (S::Prolog1, Role::Comment),
            (S::Prolog1 | S::Prolog2, T::S) => (here, Role::None),
            (S::Prolog0 | S::Prolog1, T::DeclOpen { name_end }) if decl(name_end) == b"DOCTYPE" => {
                (S::Doctype0, Role::None)
            }
            (S::Prolog0 | S::Prolog1 | S::Prolog2, T::InstanceStart) => {
                (S::Done, Role::InstanceStart)
            }
            (S::Prolog2, T::Pi { .. }) => (here, Role::Pi),
            (S::Prolog2, T::Comment) => (here, Role::Comment),

            (S::Doctype0, T::Name) => (S::Doctype1, Role::None),
            (S::Doctype1 | S::Doctype4, T::OpenBracket) => (S::InternalSubset, Role::None),
            (S::Doctype1 | S::Doctype4 | S::Doctype5, T::DeclClose) => (S::Prolog2, Role::None),
            (S::Doctype1, T::Name) if name() == b"SYSTEM" => (S::Doctype3, Role::None),
            (S::Doctype1, T::Name) if name() == b"PUBLIC" => (S::Doctype2, Role::None),
            (S::Doctype2, T::Literal) => (S::Doctype3, Role::DoctypePublicId),
            (S::Doctype3, T::Literal) => (S::Doctype4, Role::DoctypeSystemId),

            (S::InternalSubset, T::DeclOpen { name_end }) => match decl(name_end) {
                b"ENTITY" => (S::Entity0, Role::None),
                b"ATTLIST" => (S::Attlist0, Role::None),
                b"ELEMENT" => (S::Element0, Role::None),
                b"NOTATION" => (S::Notation0, Role::None),
                _ => (S::Done, Role::Error),
            },
            (S::InternalSubset, T::Pi { .. }) => (here, Role::Pi),
            (S::InternalSubset, T::Comment) => (here, Role::Comment),
            (S::InternalSubset, T::ParamEntityRef) => (here, Role::None),
            (S::InternalSubset, T::CloseBracket) => (S::Doctype5, Role::None),

            (S::Entity0, T::Percent) => (S::Entity1, Role::None),
            (S::Entity0, T::Name) => (S::Entity2, Role::GeneralEntityName),
            (S::Entity1, T::Name) => (S::Entity7, Role::ParamEntityName),
            (S::Entity2, T::Name) if name() == b"SYSTEM" => (S::Entity4, Role::None),
            (S::Entity2, T::Name) if name() == b"PUBLIC" => (S::Entity3, Role::None),
            (S::Entity7, T::Name) if name() == b"SYSTEM" => (S::Entity9, Role::None),
            (S::Entity7, T::Name) if name() == b"PUBLIC" => (S::Entity8, Role::None),
            (S::Entity2 | S::Entity7, T::Literal) => (S::DeclClose, Role::EntityValue),
            (S::Entity3, T::Literal) => (S::Entity4, Role::EntityPublicId),
            (S::Entity8, T::Literal) => (S::Entity9, Role::EntityPublicId),
            (S::Entity4, T::Literal) => (S::Entity5, Role::None),
            (S::Entity9, T::Literal) => (S::Entity10, Role::None),
            (S::Entity5 | S::Entity10, T::DeclClose) => (S::InternalSubset, Role::EntityComplete),
            (S::Entity5, T::Name) if name() == b"NDATA" => (S::Entity6, Role::None),
            (S::Entity6, T::Name) => (S::DeclClose, Role::EntityNotationName),

            (S::Notation0, T::Name) => (S::Notation1, Role::None),
            (S::Notation1, T::Name) if name() == b"SYSTEM" => (S::Notation3, Role::None),
            (S::Notation1, T::Name) if name() == b"PUBLIC" => (S::Notation2, Role::None),
            (S::Notation2, T::Literal) => (S::Notation4, Role::NotationPublicId),
            (S::Notation3 | S::Notation4, T::Literal) => (S::DeclClose, Role::None),
            (S::Notation4, T::DeclClose) => (S::InternalSubset, Role::None),

            (S::Attlist0, T::Name) => (S::Attlist1, Role::AttlistElementName),
            (S::Attlist1, T::DeclClose) => (S::InternalSubset, Role::None),
            (S::Attlist1, T::Name) => (S::Attlist2, Role::AttributeName),
            (S::Attlist2, T::Name) => match name() {
                b"CDATA" => (
                    S::Attlist8,
                    Role::AttributeType {
                        cdata: true,
                        id: false,
                    },
                ),
                b"ID" => (
                    S::Attlist8,
                    Role::AttributeType {
                        cdata: false,
                        id: true,
                    },
                ),
                b"IDREF" | b"IDREFS" | b"ENTITY" | b"ENTITIES" | b"NMTOKEN" | b"NMTOKENS" => (
                    S::Attlist8,
                    Role::AttributeType {
                        cdata: false,
                        id: false,
                    },
                ),
                b"NOTATION" => (S::Attlist5, Role::None),
                _ => (S::Done, Role::Error),
            },
            (S::Attlist2, T::OpenParen) => (S::Attlist3, Role::None),
            (S::Attlist3, T::Nmtoken | T::Name) => (S::Attlist4, Role::None),
            (S::Attlist4 | S::Attlist7, T::CloseParen) => (S::Attlist8, Role::None),
            (S::Attlist4, T::Or) => (S::Attlist3, Role::None),
            (S::Attlist5, T::OpenParen) => (S::Attlist6, Role::None),
            (S::Attlist6, T::Name) => (S::Attlist7, Role::None),
            (S::Attlist7, T::Or) => (S::Attlist6, Role::None),
            (S::Attlist8, T::PoundName) => match pound() {
                b"IMPLIED" | b"REQUIRED" => (S::Attlist1, Role::NoDefault),
                b"FIXED" => (S::Attlist9, Role::None),
                _ => (S::Done, Role::Error),
            },
            (S::Attlist8 | S::Attlist9, T::Literal) => (S::Attlist1, Role::DefaultValue),

            (S::Element0, T::Name) => (S::Element1, Role::None),
            (S::Element1, T::Name) if matches!(name(), b"EMPTY" | b"ANY") => {
                (S::DeclClose, Role::None)
            }
            (S::Element1, T::OpenParen) => {
                self.level = 1;
                (S::Element2, Role::GroupOpen)
            }
            (S::Element2, T::PoundName) if pound() == b"PCDATA" => (S::Element3, Role::None),
            (S::Element2, T::OpenParen) => {
                self.level = 2;
                (S::Element6, Role::GroupOpen)
            }
            (S::Element6, T::OpenParen) => {
                self.level += 1;
                (here, Role::GroupOpen)
            }
            (
                S::Element2 | S::Element6,
                T::Name | T::NameQuestion | T::NameAsterisk | T::NamePlus,
            ) => (S::Element7, Role::None),
            (S::Element3, T::CloseParen | T::CloseParenAsterisk) => (S::DeclClose, Role::None),
            (S::Element3 | S::Element5, T::Or) => (S::Element4, Role::None),
            (S::Element4, T::Name) => (S::Element5, Role::None),
            (S::Element5, T::CloseParenAsterisk) => (S::DeclClose, Role::None),
            (
                S::Element7,
                T::CloseParen | T::CloseParenAsterisk | T::CloseParenQuestion | T::CloseParenPlus,
            ) => {
                self.level = self.level.saturating_sub(1);
                (
                    if self.level == 0 { S::DeclClose } else { here },
                    Role::None,
                )
            }
            (S::Element7, T::Comma) => (S::Element6, Role::GroupSequence),
            (S::Element7, T::Or) => (S::Element6, Role::GroupChoice),

            (S::DeclClose, T::DeclClose) => (S::InternalSubset, Role::None),

            // White space is allowed between the tokens of every declaration.
            (
                S::Doctype0
                | S::Doctype1
                | S::Doctype2
                | S::Doctype3
                | S::Doctype4
                | S::Doctype5
                | S::InternalSubset
                | S::Entity0
                | S::Entity1
                | S::Entity2
                | S::Entity3
                | S::Entity4
                | S::Entity5
                | S::Entity6
                | S::Entity7
                | S::Entity8
                | S::Entity9
                | S::Entity10
                | S::Notation0
                | S::Notation1
                | S::Notation2
                | S::Notation3
                | S::Notation4
                | S::Attlist0
                | S::Attlist1
                | S::Attlist2
                | S::Attlist3
                | S::Attlist4
                | S::Attlist5
                | S::Attlist6
                | S::Attlist7
                | S::Attlist8
                | S::Attlist9
                | S::Element0
                | S::Element1
                | S::Element2
                | S::Element3
                | S::Element4
                | S::Element5
                | S::Element6
                | S::Element7
                | S::DeclClose,
                T::S,
            ) => (here, Role::None),
            _ => (S::Done, Role::Error),
        };
        self.state = state;
        role
    }

    pub(crate) fn level(&self) -> usize {
        self.level
    }
}

/// A span of `Dtd::arena`.
#[derive(Clone, Copy, Debug)]
struct Span {
    start: u32,
    len: u32,
}

/// No item.
const NONE: u32 = u32::MAX;

/// Items of a list found by name: their indices, placed by an FNV-1a hash of the name with
/// linear probing. Keeps a DTD with thousands of declarations fast, as expat's hash tables did.
/// The hash starts from where the first table was allocated, so a document cannot choose names
/// that all collide (expat salts its hash for the same reason); it changes placement only.
struct Index {
    slots: Vec<u32>,
    len: usize,
    seed: u32,
}

impl Index {
    const fn new() -> Self {
        Index {
            slots: Vec::new(),
            len: 0,
            seed: 0,
        }
    }

    fn hash(&self, name: &[u8]) -> usize {
        name.iter().fold(0x811C_9DC5_u32 ^ self.seed, |h, &b| {
            (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
        }) as usize
    }

    /// The item named `name`; `name_of` gives each item's name.
    fn find<'a>(&self, name: &[u8], name_of: impl Fn(u32) -> &'a [u8]) -> Option<u32> {
        let mask = self.slots.len().checked_sub(1)?;
        let mut at = self.hash(name) & mask;
        loop {
            match self.slots.get(at).copied() {
                None | Some(NONE) => return None,
                Some(item) if name_of(item) == name => return Some(item),
                Some(_) => at = (at + 1) & mask,
            }
        }
    }

    /// Adds `item`, which `find` does not have yet.
    fn insert<'a>(&mut self, item: u32, name_of: impl Fn(u32) -> &'a [u8]) -> Result<(), Error> {
        if (self.len + 1) * 2 > self.slots.len() {
            let size = (self.slots.len() * 2).max(16);
            let mut slots = Vec::new();
            if slots.try_reserve_exact(size).is_err() {
                return Err(Error::NoMemory);
            }
            slots.resize(size, NONE);
            if self.seed == 0 {
                // Truncated on purpose: any bits of the address do.
                #[allow(clippy::cast_possible_truncation)]
                let address = slots.as_ptr() as usize as u32;
                self.seed = address | 1;
            }
            let old = core::mem::replace(&mut self.slots, slots);
            for item in old.into_iter().filter(|&i| i != NONE) {
                self.place(item, &name_of);
            }
        }
        self.place(item, &name_of);
        self.len += 1;
        Ok(())
    }

    fn place<'a>(&mut self, item: u32, name_of: &impl Fn(u32) -> &'a [u8]) {
        let mask = self.slots.len() - 1;
        let mut at = self.hash(name_of(item)) & mask;
        while let Some(slot) = self.slots.get_mut(at) {
            if *slot == NONE {
                *slot = item;
                return;
            }
            at = (at + 1) & mask;
        }
    }
}

struct Entity {
    name: Span,
    /// Declared with NDATA: a reference to it is an error.
    unparsed: bool,
}

/// An element some ATTLIST names, and the first and last of its attribute declarations.
struct Element {
    name: Span,
    first: u32,
    last: u32,
}

struct AttDecl {
    attribute: Span,
    value: Option<Span>,
    cdata: bool,
    /// The element's next declaration.
    next: u32,
}

/// What the internal subset declares.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Dtd {
    arena: Vec<u8>,
    entities: Vec<Entity>,
    entity_index: Index,
    elements: Vec<Element>,
    element_index: Index,
    atts: Vec<AttDecl>,
    /// The document has an external DTD subset, so an entity it does not declare may be
    /// declared there: references to it are skipped instead of failing (unless standalone).
    pub(crate) has_external: bool,
    pub(crate) standalone: bool,
    /// The general entity being declared, when it is new.
    declaring: Option<usize>,
    element: Span,
    attribute: Span,
    cdata: bool,
    id: bool,
    /// `,` or `|` between the items of each open content model group, 0 before the first.
    connectors: Vec<u8>,
    limit: usize,
}

/// How a general entity reference resolves.
pub(crate) enum Reference {
    /// Declared: its replacement text is the reference itself (expat with `XML_GE=0`).
    Text,
    Unparsed,
    /// Neither declared nor required to be.
    Skipped,
}

fn text(arena: &[u8], span: Span) -> &[u8] {
    let start = span.start as usize;
    arena
        .get(start..start + span.len as usize)
        .unwrap_or_default()
}

fn index32(n: usize) -> Result<u32, Error> {
    u32::try_from(n)
        .ok()
        .filter(|&n| n != NONE)
        .ok_or(Error::NoMemory)
}

impl Dtd {
    pub(crate) const fn new(limit: usize) -> Self {
        Dtd {
            arena: Vec::new(),
            entities: Vec::new(),
            entity_index: Index::new(),
            elements: Vec::new(),
            element_index: Index::new(),
            atts: Vec::new(),
            has_external: false,
            standalone: false,
            declaring: None,
            element: Span { start: 0, len: 0 },
            attribute: Span { start: 0, len: 0 },
            cdata: false,
            id: false,
            connectors: Vec::new(),
            limit,
        }
    }

    fn store(&mut self, text: &[u8]) -> Result<Span, Error> {
        if self.arena.len() + text.len() > self.limit || self.arena.try_reserve(text.len()).is_err()
        {
            return Err(Error::NoMemory);
        }
        let span = Span {
            start: u32::try_from(self.arena.len()).map_err(|_| Error::NoMemory)?,
            len: u32::try_from(text.len()).map_err(|_| Error::NoMemory)?,
        };
        self.arena.extend_from_slice(text);
        Ok(span)
    }

    fn entity(&self, name: &[u8]) -> Option<&Entity> {
        let (arena, entities) = (&self.arena, &self.entities);
        let found = self.entity_index.find(name, |i| {
            entities
                .get(i as usize)
                .map_or(&[][..], |e| text(arena, e.name))
        })?;
        entities.get(found as usize)
    }

    /// `<!ENTITY name`: the first declaration of a name counts; the predefined ones are ignored.
    pub(crate) fn declare_entity(&mut self, name: &[u8]) -> Result<(), Error> {
        self.declaring = None;
        if crate::scan::predefined(name).is_some() || self.entity(name).is_some() {
            return Ok(());
        }
        let span = self.store(name)?;
        let item = index32(self.entities.len())?;
        if self.entities.try_reserve(1).is_err() {
            return Err(Error::NoMemory);
        }
        self.entities.push(Entity {
            name: span,
            unparsed: false,
        });
        let (arena, entities) = (&self.arena, &self.entities);
        self.entity_index.insert(item, |i| {
            entities
                .get(i as usize)
                .map_or(&[][..], |e| text(arena, e.name))
        })?;
        self.declaring = Some(item as usize);
        Ok(())
    }

    /// `NDATA notation` closed the entity being declared.
    pub(crate) fn unparsed_entity(&mut self) {
        if let Some(entity) = self.declaring.and_then(|i| self.entities.get_mut(i)) {
            entity.unparsed = true;
        }
    }

    /// A parameter entity declaration, which nothing uses.
    pub(crate) fn declare_parameter_entity(&mut self) {
        self.declaring = None;
    }

    pub(crate) fn reference(&self, name: &[u8]) -> Result<Reference, Error> {
        match self.entity(name) {
            Some(e) if e.unparsed => Ok(Reference::Unparsed),
            Some(_) => Ok(Reference::Text),
            None if self.has_external && !self.standalone => Ok(Reference::Skipped),
            None => Err(Error::UndefinedEntity),
        }
    }

    pub(crate) fn attlist_element(&mut self, name: &[u8]) -> Result<(), Error> {
        self.element = self.store(name)?;
        Ok(())
    }

    pub(crate) fn attribute_name(&mut self, name: &[u8]) -> Result<(), Error> {
        self.attribute = self.store(name)?;
        self.cdata = false;
        self.id = false;
        Ok(())
    }

    pub(crate) fn attribute_type(&mut self, cdata: bool, id: bool) {
        self.cdata = cdata;
        self.id = id;
    }

    /// Whether the attribute being declared is CDATA (its value keeps its spaces).
    pub(crate) fn declaring_cdata(&self) -> bool {
        self.cdata
    }

    fn element(&self, name: &[u8]) -> Option<u32> {
        let (arena, elements) = (&self.arena, &self.elements);
        self.element_index.find(name, |i| {
            elements
                .get(i as usize)
                .map_or(&[][..], |e| text(arena, e.name))
        })
    }

    /// The declarations of an element's attributes, in order.
    fn decls<'a>(&'a self, element: &[u8]) -> impl Iterator<Item = &'a AttDecl> + 'a {
        let first = self
            .element(element)
            .and_then(|e| self.elements.get(e as usize))
            .map_or(NONE, |e| e.first);
        core::iter::successors(self.atts.get(first as usize), |d| {
            self.atts.get(d.next as usize)
        })
    }

    /// The entry of the element being declared, made if it has none.
    fn element_entry(&mut self) -> Result<u32, Error> {
        if let Some(e) = self.element(text(&self.arena, self.element)) {
            return Ok(e);
        }
        let item = index32(self.elements.len())?;
        if self.elements.try_reserve(1).is_err() {
            return Err(Error::NoMemory);
        }
        self.elements.push(Element {
            name: self.element,
            first: NONE,
            last: NONE,
        });
        let (arena, elements) = (&self.arena, &self.elements);
        self.element_index.insert(item, |i| {
            elements
                .get(i as usize)
                .map_or(&[][..], |e| text(arena, e.name))
        })?;
        Ok(item)
    }

    /// Defines the attribute being declared, with a default value or none. A default or an ID
    /// counts only when the element does not have the attribute yet.
    pub(crate) fn define_attribute(&mut self, value: Option<&[u8]>) -> Result<(), Error> {
        let id = self.id && value.is_none();
        let element_name = text(&self.arena, self.element);
        let attribute_name = text(&self.arena, self.attribute);
        if (value.is_some() || id)
            && self
                .decls(element_name)
                .any(|d| text(&self.arena, d.attribute) == attribute_name)
        {
            return Ok(());
        }
        let element = self.element_entry()?;
        let value = match value {
            Some(v) => Some(self.store(v)?),
            None => None,
        };
        let decl = index32(self.atts.len())?;
        if self.atts.try_reserve(1).is_err() {
            return Err(Error::NoMemory);
        }
        self.atts.push(AttDecl {
            attribute: self.attribute,
            value,
            cdata: self.cdata,
            next: NONE,
        });
        let Some(entry) = self.elements.get_mut(element as usize) else {
            return Ok(());
        };
        let last = core::mem::replace(&mut entry.last, decl);
        if last == NONE {
            entry.first = decl;
        } else if let Some(previous) = self.atts.get_mut(last as usize) {
            previous.next = decl;
        }
        Ok(())
    }

    /// Whether an attribute of an element is CDATA, as its first declaration says.
    pub(crate) fn is_cdata(&self, element: &[u8], attribute: &[u8]) -> bool {
        if self.atts.is_empty() {
            return true;
        }
        self.decls(element)
            .find(|d| text(&self.arena, d.attribute) == attribute)
            .is_none_or(|d| d.cdata)
    }

    /// The default values of an element's attributes, in declaration order.
    pub(crate) fn defaults<'a>(
        &'a self,
        element: &'a [u8],
    ) -> impl Iterator<Item = (&'a [u8], &'a [u8])> + 'a {
        self.decls(element)
            .filter_map(|d| Some((text(&self.arena, d.attribute), text(&self.arena, d.value?))))
    }

    pub(crate) fn has_attlists(&self) -> bool {
        !self.atts.is_empty()
    }

    /// A content model group opened at `level`.
    pub(crate) fn group_open(&mut self, level: usize) -> Result<(), Error> {
        if level > self.limit {
            return Err(Error::NoMemory);
        }
        if self.connectors.len() <= level {
            if self
                .connectors
                .try_reserve(level + 1 - self.connectors.len())
                .is_err()
            {
                return Err(Error::NoMemory);
            }
            self.connectors.resize(level + 1, 0);
        }
        if let Some(c) = self.connectors.get_mut(level) {
            *c = 0;
        }
        Ok(())
    }

    /// `,` or `|` in the group at `level`; a group may not mix them.
    pub(crate) fn connector(&mut self, level: usize, c: u8) -> Result<(), Error> {
        let other = if c == b',' { b'|' } else { b',' };
        match self.connectors.get_mut(level) {
            Some(slot) if *slot == other => Err(Error::Syntax),
            Some(slot) => {
                *slot = c;
                Ok(())
            }
            None => Ok(()),
        }
    }
}
