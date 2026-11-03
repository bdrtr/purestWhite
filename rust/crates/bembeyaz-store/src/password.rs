//! A stored password or other secret read back (`lib/Serialization/StoredPassword.cpp`'s
//! `obfuscation::readStoredPassword`, `ObfuscationUtils`' `deobfuscateFromBase64`), with the
//! decoder given: the C++ firmware's (`ObfuscationUtils`, keyed by the eFuse MAC), the bare-metal
//! firmware's, or a test's stand-in. A plain secret is held in a [`Secret`], wiped when dropped.

use alloc::vec::Vec;
use bembeyaz_jsondoc::Error;
use core::fmt;
use core::ops::Deref;

/// A plain secret (a password, a header's value): its bytes are wiped when it is dropped, and
/// `Debug` shows only its length. Copies made elsewhere (the file's text, the parsed file, C++'s
/// strings) are not this type's to wipe.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(Vec<u8>);

impl Secret {
    /// `bytes`, now held as a secret.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Secret {
        Secret(bytes)
    }

    /// A copy of `bytes`.
    ///
    /// # Errors
    /// [`Error::NoMemory`] when memory runs out.
    pub fn copy_of(bytes: &[u8]) -> Result<Secret, Error> {
        crate::copied(bytes).map(Secret)
    }
}

impl Deref for Secret {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // The spare capacity too (it may hold bytes from before a truncate): resize stays within it.
        let capacity = self.0.capacity();
        self.0.resize(capacity, 0);
        self.0.fill(0);
        // The zeros are never read: this keeps the compiler from leaving the fill out (best effort,
        // without unsafe code).
        core::hint::black_box(&self.0);
    }
}

/// Its length only.
impl fmt::Debug for Secret {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "Secret({} bytes)", self.0.len())
    }
}

/// What decoding a stored secret gave (`obfuscation::deobfuscateFromBase64`): the text, whether
/// it decoded, and whether it was longer than asked for (then the text is empty).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decoded {
    pub text: Secret,
    pub ok: bool,
    pub too_long: bool,
}

/// The password (`obfuscation::readStoredPassword`): `obfuscated` decoded, else the legacy plain
/// one, which asks for the file to be saved again (`resave`) when it is not empty; none when either
/// is longer than `most`. `decode(text, most)` decodes a stored secret of at most `most` bytes.
///
/// # Errors
/// [`Error::NoMemory`] when the decoder or the legacy password's copy runs out of memory.
pub fn stored(
    obfuscated: &[u8],
    legacy: &[u8],
    most: usize,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
    resave: &mut bool,
) -> Result<Option<Secret>, Error> {
    let decoded = decode(obfuscated, most)?;
    if decoded.too_long {
        return Ok(None);
    }
    if decoded.ok {
        return Ok(Some(decoded.text));
    }
    if legacy.len() > most {
        return Ok(None);
    }
    *resave |= !legacy.is_empty();
    Secret::copy_of(legacy).map(Some)
}

/// A stored secret decoded with no length limit (`deobfuscateFromBase64(text)`): empty when it does
/// not decode.
///
/// # Errors
/// [`Error::NoMemory`] when the decoder runs out of memory.
pub fn decoded(
    obfuscated: &[u8],
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Secret, Error> {
    let decoded = decode(obfuscated, usize::MAX)?;
    Ok(if decoded.ok {
        decoded.text
    } else {
        Secret::default()
    })
}
