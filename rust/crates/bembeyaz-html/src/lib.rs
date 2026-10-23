//! HTML named character references, as lib/Epub/Epub/htmlEntities.cpp listed them up to e82f2461
//! (the table came from
//! <https://github.com/atomic14/diy-esp32-epub-reader/blob/2c2f57fdd7e2a788d14a0bcb26b9e845a47aac42/lib/Epub/RubbishHtmlParser/htmlEntities.cpp>):
//! the 252 of HTML 4 plus `&apos;`.
//!
//! Each entry is a name of at most 8 bytes and a UTF-8 value of at most 3, both zero-padded: 12
//! bytes with no pointers, and every value is followed by a NUL for the C++ API that returns it as
//! a C string.
//!
//! [`text`] turns a plain text book into an XHTML body; [`words`] cuts a chapter's text into words;
//! [`element`] classifies a chapter's elements.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod element;
pub mod text;
pub mod words;

/// An entity's value.
#[derive(Clone, Copy)]
pub struct Value(&'static [u8; 4]);

impl Value {
    /// The UTF-8 text.
    #[must_use]
    pub fn text(self) -> &'static [u8] {
        let len = self.0.iter().position(|&b| b == 0).unwrap_or(self.0.len());
        self.0.get(..len).unwrap_or_default()
    }

    /// The text followed by its NUL, as a C string.
    #[must_use]
    pub fn as_c_str(self) -> *const u8 {
        self.0.as_ptr()
    }
}

/// Elements whose text is not shown: `head`, `style`, `script`, `title` and `rp`, in any case.
#[must_use]
pub fn is_non_visible_element(name: &[u8]) -> bool {
    [&b"head"[..], b"style", b"script", b"title", b"rp"]
        .iter()
        .any(|tag| name.eq_ignore_ascii_case(tag))
}

/// The value of the entity `name` (without `&` and `;`), case-sensitive.
#[must_use]
pub fn entity(name: &[u8]) -> Option<Value> {
    TABLE
        .binary_search_by(|(key, _)| trim(key).cmp(name))
        .ok()
        .and_then(|i| TABLE.get(i))
        .map(|(_, value)| Value(value))
}

/// A zero-padded name without its padding.
fn trim(key: &[u8; 8]) -> &[u8] {
    let len = key.iter().position(|&b| b == 0).unwrap_or(key.len());
    key.get(..len).unwrap_or_default()
}

// Sorted by name, for binary search; each value has a NUL in its last byte (checked by a test).
static TABLE: [([u8; 8], [u8; 4]); 253] = [
    (*b"AElig\0\0\0", *b"\xc3\x86\0\0"),
    (*b"Aacute\0\0", *b"\xc3\x81\0\0"),
    (*b"Acirc\0\0\0", *b"\xc3\x82\0\0"),
    (*b"Agrave\0\0", *b"\xc3\x80\0\0"),
    (*b"Alpha\0\0\0", *b"\xce\x91\0\0"),
    (*b"Aring\0\0\0", *b"\xc3\x85\0\0"),
    (*b"Atilde\0\0", *b"\xc3\x83\0\0"),
    (*b"Auml\0\0\0\0", *b"\xc3\x84\0\0"),
    (*b"Beta\0\0\0\0", *b"\xce\x92\0\0"),
    (*b"Ccedil\0\0", *b"\xc3\x87\0\0"),
    (*b"Chi\0\0\0\0\0", *b"\xce\xa7\0\0"),
    (*b"Dagger\0\0", *b"\xe2\x80\xa1\0"),
    (*b"Delta\0\0\0", *b"\xce\x94\0\0"),
    (*b"ETH\0\0\0\0\0", *b"\xc3\x90\0\0"),
    (*b"Eacute\0\0", *b"\xc3\x89\0\0"),
    (*b"Ecirc\0\0\0", *b"\xc3\x8a\0\0"),
    (*b"Egrave\0\0", *b"\xc3\x88\0\0"),
    (*b"Epsilon\0", *b"\xce\x95\0\0"),
    (*b"Eta\0\0\0\0\0", *b"\xce\x97\0\0"),
    (*b"Euml\0\0\0\0", *b"\xc3\x8b\0\0"),
    (*b"Gamma\0\0\0", *b"\xce\x93\0\0"),
    (*b"Iacute\0\0", *b"\xc3\x8d\0\0"),
    (*b"Icirc\0\0\0", *b"\xc3\x8e\0\0"),
    (*b"Igrave\0\0", *b"\xc3\x8c\0\0"),
    (*b"Iota\0\0\0\0", *b"\xce\x99\0\0"),
    (*b"Iuml\0\0\0\0", *b"\xc3\x8f\0\0"),
    (*b"Kappa\0\0\0", *b"\xce\x9a\0\0"),
    (*b"Lambda\0\0", *b"\xce\x9b\0\0"),
    (*b"Mu\0\0\0\0\0\0", *b"\xce\x9c\0\0"),
    (*b"Ntilde\0\0", *b"\xc3\x91\0\0"),
    (*b"Nu\0\0\0\0\0\0", *b"\xce\x9d\0\0"),
    (*b"OElig\0\0\0", *b"\xc5\x92\0\0"),
    (*b"Oacute\0\0", *b"\xc3\x93\0\0"),
    (*b"Ocirc\0\0\0", *b"\xc3\x94\0\0"),
    (*b"Ograve\0\0", *b"\xc3\x92\0\0"),
    (*b"Omega\0\0\0", *b"\xce\xa9\0\0"),
    (*b"Omicron\0", *b"\xce\x9f\0\0"),
    (*b"Oslash\0\0", *b"\xc3\x98\0\0"),
    (*b"Otilde\0\0", *b"\xc3\x95\0\0"),
    (*b"Ouml\0\0\0\0", *b"\xc3\x96\0\0"),
    (*b"Phi\0\0\0\0\0", *b"\xce\xa6\0\0"),
    (*b"Pi\0\0\0\0\0\0", *b"\xce\xa0\0\0"),
    (*b"Prime\0\0\0", *b"\xe2\x80\xb3\0"),
    (*b"Psi\0\0\0\0\0", *b"\xce\xa8\0\0"),
    (*b"Rho\0\0\0\0\0", *b"\xce\xa1\0\0"),
    (*b"Scaron\0\0", *b"\xc5\xa0\0\0"),
    (*b"Sigma\0\0\0", *b"\xce\xa3\0\0"),
    (*b"THORN\0\0\0", *b"\xc3\x9e\0\0"),
    (*b"Tau\0\0\0\0\0", *b"\xce\xa4\0\0"),
    (*b"Theta\0\0\0", *b"\xce\x98\0\0"),
    (*b"Uacute\0\0", *b"\xc3\x9a\0\0"),
    (*b"Ucirc\0\0\0", *b"\xc3\x9b\0\0"),
    (*b"Ugrave\0\0", *b"\xc3\x99\0\0"),
    (*b"Upsilon\0", *b"\xce\xa5\0\0"),
    (*b"Uuml\0\0\0\0", *b"\xc3\x9c\0\0"),
    (*b"Xi\0\0\0\0\0\0", *b"\xce\x9e\0\0"),
    (*b"Yacute\0\0", *b"\xc3\x9d\0\0"),
    (*b"Yuml\0\0\0\0", *b"\xc5\xb8\0\0"),
    (*b"Zeta\0\0\0\0", *b"\xce\x96\0\0"),
    (*b"aacute\0\0", *b"\xc3\xa1\0\0"),
    (*b"acirc\0\0\0", *b"\xc3\xa2\0\0"),
    (*b"acute\0\0\0", *b"\xc2\xb4\0\0"),
    (*b"aelig\0\0\0", *b"\xc3\xa6\0\0"),
    (*b"agrave\0\0", *b"\xc3\xa0\0\0"),
    (*b"alefsym\0", *b"\xe2\x84\xb5\0"),
    (*b"alpha\0\0\0", *b"\xce\xb1\0\0"),
    (*b"amp\0\0\0\0\0", *b"&\0\0\0"),
    (*b"and\0\0\0\0\0", *b"\xe2\x88\xa7\0"),
    (*b"ang\0\0\0\0\0", *b"\xe2\x88\xa0\0"),
    (*b"apos\0\0\0\0", *b"'\0\0\0"),
    (*b"aring\0\0\0", *b"\xc3\xa5\0\0"),
    (*b"asymp\0\0\0", *b"\xe2\x89\x88\0"),
    (*b"atilde\0\0", *b"\xc3\xa3\0\0"),
    (*b"auml\0\0\0\0", *b"\xc3\xa4\0\0"),
    (*b"bdquo\0\0\0", *b"\xe2\x80\x9e\0"),
    (*b"beta\0\0\0\0", *b"\xce\xb2\0\0"),
    (*b"brvbar\0\0", *b"\xc2\xa6\0\0"),
    (*b"bull\0\0\0\0", *b"\xe2\x80\xa2\0"),
    (*b"cap\0\0\0\0\0", *b"\xe2\x88\xa9\0"),
    (*b"ccedil\0\0", *b"\xc3\xa7\0\0"),
    (*b"cedil\0\0\0", *b"\xc2\xb8\0\0"),
    (*b"cent\0\0\0\0", *b"\xc2\xa2\0\0"),
    (*b"chi\0\0\0\0\0", *b"\xcf\x87\0\0"),
    (*b"circ\0\0\0\0", *b"\xcb\x86\0\0"),
    (*b"clubs\0\0\0", *b"\xe2\x99\xa3\0"),
    (*b"cong\0\0\0\0", *b"\xe2\x89\x85\0"),
    (*b"copy\0\0\0\0", *b"\xc2\xa9\0\0"),
    (*b"crarr\0\0\0", *b"\xe2\x86\xb5\0"),
    (*b"cup\0\0\0\0\0", *b"\xe2\x88\xaa\0"),
    (*b"curren\0\0", *b"\xc2\xa4\0\0"),
    (*b"dArr\0\0\0\0", *b"\xe2\x87\x93\0"),
    (*b"dagger\0\0", *b"\xe2\x80\xa0\0"),
    (*b"darr\0\0\0\0", *b"\xe2\x86\x93\0"),
    (*b"deg\0\0\0\0\0", *b"\xc2\xb0\0\0"),
    (*b"delta\0\0\0", *b"\xce\xb4\0\0"),
    (*b"diams\0\0\0", *b"\xe2\x99\xa6\0"),
    (*b"divide\0\0", *b"\xc3\xb7\0\0"),
    (*b"eacute\0\0", *b"\xc3\xa9\0\0"),
    (*b"ecirc\0\0\0", *b"\xc3\xaa\0\0"),
    (*b"egrave\0\0", *b"\xc3\xa8\0\0"),
    (*b"empty\0\0\0", *b"\xe2\x88\x85\0"),
    (*b"emsp\0\0\0\0", *b" \0\0\0"),
    (*b"ensp\0\0\0\0", *b" \0\0\0"),
    (*b"epsilon\0", *b"\xce\xb5\0\0"),
    (*b"equiv\0\0\0", *b"\xe2\x89\xa1\0"),
    (*b"eta\0\0\0\0\0", *b"\xce\xb7\0\0"),
    (*b"eth\0\0\0\0\0", *b"\xc3\xb0\0\0"),
    (*b"euml\0\0\0\0", *b"\xc3\xab\0\0"),
    (*b"euro\0\0\0\0", *b"\xe2\x82\xac\0"),
    (*b"exist\0\0\0", *b"\xe2\x88\x83\0"),
    (*b"fnof\0\0\0\0", *b"\xc6\x92\0\0"),
    (*b"forall\0\0", *b"\xe2\x88\x80\0"),
    (*b"frac12\0\0", *b"\xc2\xbd\0\0"),
    (*b"frac14\0\0", *b"\xc2\xbc\0\0"),
    (*b"frac34\0\0", *b"\xc2\xbe\0\0"),
    (*b"frasl\0\0\0", *b"\xe2\x81\x84\0"),
    (*b"gamma\0\0\0", *b"\xce\xb3\0\0"),
    (*b"ge\0\0\0\0\0\0", *b"\xe2\x89\xa5\0"),
    (*b"gt\0\0\0\0\0\0", *b">\0\0\0"),
    (*b"hArr\0\0\0\0", *b"\xe2\x87\x94\0"),
    (*b"harr\0\0\0\0", *b"\xe2\x86\x94\0"),
    (*b"hearts\0\0", *b"\xe2\x99\xa5\0"),
    (*b"hellip\0\0", *b"\xe2\x80\xa6\0"),
    (*b"iacute\0\0", *b"\xc3\xad\0\0"),
    (*b"icirc\0\0\0", *b"\xc3\xae\0\0"),
    (*b"iexcl\0\0\0", *b"\xc2\xa1\0\0"),
    (*b"igrave\0\0", *b"\xc3\xac\0\0"),
    (*b"image\0\0\0", *b"\xe2\x84\x91\0"),
    (*b"infin\0\0\0", *b"\xe2\x88\x9e\0"),
    (*b"int\0\0\0\0\0", *b"\xe2\x88\xab\0"),
    (*b"iota\0\0\0\0", *b"\xce\xb9\0\0"),
    (*b"iquest\0\0", *b"\xc2\xbf\0\0"),
    (*b"isin\0\0\0\0", *b"\xe2\x88\x88\0"),
    (*b"iuml\0\0\0\0", *b"\xc3\xaf\0\0"),
    (*b"kappa\0\0\0", *b"\xce\xba\0\0"),
    (*b"lArr\0\0\0\0", *b"\xe2\x87\x90\0"),
    (*b"lambda\0\0", *b"\xce\xbb\0\0"),
    (*b"lang\0\0\0\0", *b"\xe3\x80\x88\0"),
    (*b"laquo\0\0\0", *b"\xc2\xab\0\0"),
    (*b"larr\0\0\0\0", *b"\xe2\x86\x90\0"),
    (*b"lceil\0\0\0", *b"\xe2\x8c\x88\0"),
    (*b"ldquo\0\0\0", *b"\xe2\x80\x9c\0"),
    (*b"le\0\0\0\0\0\0", *b"\xe2\x89\xa4\0"),
    (*b"lfloor\0\0", *b"\xe2\x8c\x8a\0"),
    (*b"lowast\0\0", *b"\xe2\x88\x97\0"),
    (*b"loz\0\0\0\0\0", *b"\xe2\x97\x8a\0"),
    (*b"lrm\0\0\0\0\0", *b"\xe2\x80\x8e\0"),
    (*b"lsaquo\0\0", *b"\xe2\x80\xb9\0"),
    (*b"lsquo\0\0\0", *b"\xe2\x80\x98\0"),
    (*b"lt\0\0\0\0\0\0", *b"<\0\0\0"),
    (*b"macr\0\0\0\0", *b"\xc2\xaf\0\0"),
    (*b"mdash\0\0\0", *b"\xe2\x80\x94\0"),
    (*b"micro\0\0\0", *b"\xc2\xb5\0\0"),
    (*b"middot\0\0", *b"\xc2\xb7\0\0"),
    (*b"minus\0\0\0", *b"\xe2\x88\x92\0"),
    (*b"mu\0\0\0\0\0\0", *b"\xce\xbc\0\0"),
    (*b"nabla\0\0\0", *b"\xe2\x88\x87\0"),
    (*b"nbsp\0\0\0\0", *b"\xc2\xa0\0\0"),
    (*b"ndash\0\0\0", *b"\xe2\x80\x93\0"),
    (*b"ne\0\0\0\0\0\0", *b"\xe2\x89\xa0\0"),
    (*b"ni\0\0\0\0\0\0", *b"\xe2\x88\x8b\0"),
    (*b"not\0\0\0\0\0", *b"\xc2\xac\0\0"),
    (*b"notin\0\0\0", *b"\xe2\x88\x89\0"),
    (*b"nsub\0\0\0\0", *b"\xe2\x8a\x84\0"),
    (*b"ntilde\0\0", *b"\xc3\xb1\0\0"),
    (*b"nu\0\0\0\0\0\0", *b"\xce\xbd\0\0"),
    (*b"oacute\0\0", *b"\xc3\xb3\0\0"),
    (*b"ocirc\0\0\0", *b"\xc3\xb4\0\0"),
    (*b"oelig\0\0\0", *b"\xc5\x93\0\0"),
    (*b"ograve\0\0", *b"\xc3\xb2\0\0"),
    (*b"oline\0\0\0", *b"\xe2\x80\xbe\0"),
    (*b"omega\0\0\0", *b"\xcf\x89\0\0"),
    (*b"omicron\0", *b"\xce\xbf\0\0"),
    (*b"oplus\0\0\0", *b"\xe2\x8a\x95\0"),
    (*b"or\0\0\0\0\0\0", *b"\xe2\x88\xa8\0"),
    (*b"ordf\0\0\0\0", *b"\xc2\xaa\0\0"),
    (*b"ordm\0\0\0\0", *b"\xc2\xba\0\0"),
    (*b"oslash\0\0", *b"\xc3\xb8\0\0"),
    (*b"otilde\0\0", *b"\xc3\xb5\0\0"),
    (*b"otimes\0\0", *b"\xe2\x8a\x97\0"),
    (*b"ouml\0\0\0\0", *b"\xc3\xb6\0\0"),
    (*b"para\0\0\0\0", *b"\xc2\xb6\0\0"),
    (*b"part\0\0\0\0", *b"\xe2\x88\x82\0"),
    (*b"permil\0\0", *b"\xe2\x80\xb0\0"),
    (*b"perp\0\0\0\0", *b"\xe2\x8a\xa5\0"),
    (*b"phi\0\0\0\0\0", *b"\xcf\x86\0\0"),
    (*b"pi\0\0\0\0\0\0", *b"\xcf\x80\0\0"),
    (*b"piv\0\0\0\0\0", *b"\xcf\x96\0\0"),
    (*b"plusmn\0\0", *b"\xc2\xb1\0\0"),
    (*b"pound\0\0\0", *b"\xc2\xa3\0\0"),
    (*b"prime\0\0\0", *b"\xe2\x80\xb2\0"),
    (*b"prod\0\0\0\0", *b"\xe2\x88\x8f\0"),
    (*b"prop\0\0\0\0", *b"\xe2\x88\x9d\0"),
    (*b"psi\0\0\0\0\0", *b"\xcf\x88\0\0"),
    (*b"quot\0\0\0\0", *b"\x22\0\0\0"),
    (*b"rArr\0\0\0\0", *b"\xe2\x87\x92\0"),
    (*b"radic\0\0\0", *b"\xe2\x88\x9a\0"),
    (*b"rang\0\0\0\0", *b"\xe3\x80\x89\0"),
    (*b"raquo\0\0\0", *b"\xc2\xbb\0\0"),
    (*b"rarr\0\0\0\0", *b"\xe2\x86\x92\0"),
    (*b"rceil\0\0\0", *b"\xe2\x8c\x89\0"),
    (*b"rdquo\0\0\0", *b"\xe2\x80\x9d\0"),
    (*b"real\0\0\0\0", *b"\xe2\x84\x9c\0"),
    (*b"reg\0\0\0\0\0", *b"\xc2\xae\0\0"),
    (*b"rfloor\0\0", *b"\xe2\x8c\x8b\0"),
    (*b"rho\0\0\0\0\0", *b"\xcf\x81\0\0"),
    (*b"rlm\0\0\0\0\0", *b"\xe2\x80\x8f\0"),
    (*b"rsaquo\0\0", *b"\xe2\x80\xba\0"),
    (*b"rsquo\0\0\0", *b"\xe2\x80\x99\0"),
    (*b"sbquo\0\0\0", *b"\xe2\x80\x9a\0"),
    (*b"scaron\0\0", *b"\xc5\xa1\0\0"),
    (*b"sdot\0\0\0\0", *b"\xe2\x8b\x85\0"),
    (*b"sect\0\0\0\0", *b"\xc2\xa7\0\0"),
    (*b"shy\0\0\0\0\0", *b"\xc2\xad\0\0"),
    (*b"sigma\0\0\0", *b"\xcf\x83\0\0"),
    (*b"sigmaf\0\0", *b"\xcf\x82\0\0"),
    (*b"sim\0\0\0\0\0", *b"\xe2\x88\xbc\0"),
    (*b"spades\0\0", *b"\xe2\x99\xa0\0"),
    (*b"sub\0\0\0\0\0", *b"\xe2\x8a\x82\0"),
    (*b"sube\0\0\0\0", *b"\xe2\x8a\x86\0"),
    (*b"sum\0\0\0\0\0", *b"\xe2\x88\x91\0"),
    (*b"sup\0\0\0\0\0", *b"\xe2\x8a\x83\0"),
    (*b"sup1\0\0\0\0", *b"\xc2\xb9\0\0"),
    (*b"sup2\0\0\0\0", *b"\xc2\xb2\0\0"),
    (*b"sup3\0\0\0\0", *b"\xc2\xb3\0\0"),
    (*b"supe\0\0\0\0", *b"\xe2\x8a\x87\0"),
    (*b"szlig\0\0\0", *b"\xc3\x9f\0\0"),
    (*b"tau\0\0\0\0\0", *b"\xcf\x84\0\0"),
    (*b"there4\0\0", *b"\xe2\x88\xb4\0"),
    (*b"theta\0\0\0", *b"\xce\xb8\0\0"),
    (*b"thetasym", *b"\xcf\x91\0\0"),
    (*b"thinsp\0\0", *b" \0\0\0"),
    (*b"thorn\0\0\0", *b"\xc3\xbe\0\0"),
    (*b"tilde\0\0\0", *b"\xcb\x9c\0\0"),
    (*b"times\0\0\0", *b"\xc3\x97\0\0"),
    (*b"trade\0\0\0", *b"\xe2\x84\xa2\0"),
    (*b"uArr\0\0\0\0", *b"\xe2\x87\x91\0"),
    (*b"uacute\0\0", *b"\xc3\xba\0\0"),
    (*b"uarr\0\0\0\0", *b"\xe2\x86\x91\0"),
    (*b"ucirc\0\0\0", *b"\xc3\xbb\0\0"),
    (*b"ugrave\0\0", *b"\xc3\xb9\0\0"),
    (*b"uml\0\0\0\0\0", *b"\xc2\xa8\0\0"),
    (*b"upsih\0\0\0", *b"\xcf\x92\0\0"),
    (*b"upsilon\0", *b"\xcf\x85\0\0"),
    (*b"uuml\0\0\0\0", *b"\xc3\xbc\0\0"),
    (*b"weierp\0\0", *b"\xe2\x84\x98\0"),
    (*b"xi\0\0\0\0\0\0", *b"\xce\xbe\0\0"),
    (*b"yacute\0\0", *b"\xc3\xbd\0\0"),
    (*b"yen\0\0\0\0\0", *b"\xc2\xa5\0\0"),
    (*b"yuml\0\0\0\0", *b"\xc3\xbf\0\0"),
    (*b"zeta\0\0\0\0", *b"\xce\xb6\0\0"),
    (*b"zwj\0\0\0\0\0", *b"\xe2\x80\x8d\0"),
    (*b"zwnj\0\0\0\0", *b"\xe2\x80\x8c\0"),
];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::{TABLE, entity, trim};

    #[test]
    fn the_table_is_sorted_and_every_value_is_a_c_string() {
        assert!(
            TABLE
                .windows(2)
                .all(|pair| trim(&pair[0].0) < trim(&pair[1].0))
        );
        assert!(
            TABLE
                .iter()
                .all(|(key, value)| !trim(key).is_empty() && value[0] != 0 && value[3] == 0)
        );
    }

    #[test]
    fn finds_entities_by_exact_name() {
        let text = |name: &[u8]| core::str::from_utf8(entity(name).unwrap().text()).unwrap();
        assert_eq!(text(b"nbsp"), "\u{a0}");
        assert_eq!(text(b"amp"), "&");
        assert_eq!(text(b"quot"), "\"");
        assert_eq!(text(b"apos"), "'");
        assert_eq!(text(b"AElig"), "Æ");
        assert_eq!(text(b"aelig"), "æ");
        assert_eq!(text(b"sup"), "⊃");
        assert_eq!(text(b"sup1"), "¹");
        assert_eq!(text(b"thetasym"), "ϑ");
        assert_eq!(text(b"zwnj"), "\u{200c}");
        for missing in [
            &b"bogus"[..],
            b"nbs",
            b"nbspx",
            b"",
            b"NBSP",
            b"thetasymx",
            b"nbsp\0",
        ] {
            assert!(entity(missing).is_none());
        }
    }
}
