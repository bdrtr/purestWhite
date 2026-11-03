//! An X.509 certificate (RFC 5280) read for what the bundle check needs: the signed part, the
//! issuer and subject names as encoded, the dates, the subject's public key and the signature.

use crate::der::{self, BIT_STRING, INTEGER, OID, SEQUENCE, VERSION};
use crate::time::Time;

/// A digest a signature is made over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hash {
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
}

/// The kind of key a signature needs, or a key is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    /// RSA, PKCS #1 v1.5 signatures.
    Rsa,
    /// ECDSA.
    Ec,
}

/// A certificate's signature algorithm, of those this reads (`purestwhite-tls` refuses
/// SHA-224).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignatureAlgorithm {
    pub key: KeyKind,
    pub hash: Hash,
}

/// The OIDs (their content bytes) of the signature algorithms read.
const SIGNATURES: [(&[u8], KeyKind, Hash); 10] = [
    // 1.2.840.113549.1.1.{5,14,11,12,13}: shaNWithRSAEncryption.
    (
        &[42, 134, 72, 134, 247, 13, 1, 1, 5],
        KeyKind::Rsa,
        Hash::Sha1,
    ),
    (
        &[42, 134, 72, 134, 247, 13, 1, 1, 14],
        KeyKind::Rsa,
        Hash::Sha224,
    ),
    (
        &[42, 134, 72, 134, 247, 13, 1, 1, 11],
        KeyKind::Rsa,
        Hash::Sha256,
    ),
    (
        &[42, 134, 72, 134, 247, 13, 1, 1, 12],
        KeyKind::Rsa,
        Hash::Sha384,
    ),
    (
        &[42, 134, 72, 134, 247, 13, 1, 1, 13],
        KeyKind::Rsa,
        Hash::Sha512,
    ),
    // 1.2.840.10045.4.1 and 1.2.840.10045.4.3.{1,2,3,4}: ecdsa-with-SHAN.
    (&[42, 134, 72, 206, 61, 4, 1], KeyKind::Ec, Hash::Sha1),
    (&[42, 134, 72, 206, 61, 4, 3, 1], KeyKind::Ec, Hash::Sha224),
    (&[42, 134, 72, 206, 61, 4, 3, 2], KeyKind::Ec, Hash::Sha256),
    (&[42, 134, 72, 206, 61, 4, 3, 3], KeyKind::Ec, Hash::Sha384),
    (&[42, 134, 72, 206, 61, 4, 3, 4], KeyKind::Ec, Hash::Sha512),
];
/// 1.2.840.113549.1.1.1 rsaEncryption and 1.2.840.10045.2.1 id-ecPublicKey.
const RSA_KEY: &[u8] = &[42, 134, 72, 134, 247, 13, 1, 1, 1];
const EC_KEY: &[u8] = &[42, 134, 72, 206, 61, 2, 1];

/// A certificate, its parts borrowed from its DER.
#[derive(Clone, Copy, Debug)]
pub struct Certificate<'a> {
    /// The signed part (`tbsCertificate`), as encoded.
    pub signed: &'a [u8],
    /// The issuer's name, as encoded (tag and length too): what the bundle is searched by.
    pub issuer: &'a [u8],
    /// The subject's name, as encoded.
    pub subject: &'a [u8],
    pub not_before: Time,
    pub not_after: Time,
    /// The subject's public key (`SubjectPublicKeyInfo`), as encoded.
    pub public_key: &'a [u8],
    /// The signature's algorithm OID, its content bytes.
    pub signature_oid: &'a [u8],
    /// The signature's bytes.
    pub signature: &'a [u8],
}

impl<'a> Certificate<'a> {
    /// `der` read; none when it is not a certificate this reads. `der` must hold exactly one
    /// certificate, nothing after it.
    #[must_use]
    pub fn read(der: &'a [u8]) -> Option<Certificate<'a>> {
        let (cert, rest) = der::expect(der, SEQUENCE)?;
        if !rest.is_empty() {
            return None;
        }
        let (signed, after) = der::expect(cert.content, SEQUENCE)?;
        let (algorithm, after) = der::expect(after, SEQUENCE)?;
        let (signature, after) = der::expect(after, BIT_STRING)?;
        if !after.is_empty() {
            return None;
        }
        let (oid, _) = der::expect(algorithm.content, OID)?;
        // A signature is a whole number of bytes: no unused bits.
        let ([0], signature) = signature.content.split_at_checked(1)? else {
            return None;
        };
        let mut cert = read_signed(signed.content)?;
        cert.signed = signed.whole;
        cert.signature_oid = oid.content;
        cert.signature = signature;
        Some(cert)
    }

    /// Whether `now` is within its dates (both ends included).
    #[must_use]
    pub fn is_current(&self, now: Time) -> bool {
        self.not_before <= now && now <= self.not_after
    }

    /// Its signature's algorithm; none for another (RSASSA-PSS, say).
    #[must_use]
    pub fn signature_algorithm(&self) -> Option<SignatureAlgorithm> {
        SIGNATURES
            .iter()
            .find(|(oid, ..)| *oid == self.signature_oid)
            .map(|&(_, key, hash)| SignatureAlgorithm { key, hash })
    }
}

/// The kind of the public key in `spki` (a `SubjectPublicKeyInfo`); none for another.
#[must_use]
pub fn key_kind(spki: &[u8]) -> Option<KeyKind> {
    let (info, _) = der::expect(spki, SEQUENCE)?;
    let (algorithm, _) = der::expect(info.content, SEQUENCE)?;
    let (oid, _) = der::expect(algorithm.content, OID)?;
    match oid.content {
        RSA_KEY => Some(KeyKind::Rsa),
        EC_KEY => Some(KeyKind::Ec),
        _ => None,
    }
}

/// The parts of `tbsCertificate`'s content: the version (optional), serial, signature algorithm,
/// issuer, validity, subject and key; what follows them is not read.
fn read_signed(tbs: &[u8]) -> Option<Certificate<'_>> {
    let rest = match der::next(tbs)? {
        (e, rest) if e.tag == VERSION => rest,
        _ => tbs,
    };
    let (_, rest) = der::expect(rest, INTEGER)?;
    let (_, rest) = der::expect(rest, SEQUENCE)?;
    let (issuer, rest) = der::expect(rest, SEQUENCE)?;
    let (validity, rest) = der::expect(rest, SEQUENCE)?;
    let (subject, rest) = der::expect(rest, SEQUENCE)?;
    let (public_key, _) = der::expect(rest, SEQUENCE)?;
    let (not_before, dates) = der::next(validity.content)?;
    let (not_after, _) = der::next(dates)?;
    Some(Certificate {
        signed: &[],
        issuer: issuer.whole,
        subject: subject.whole,
        not_before: Time::read(&not_before)?,
        not_after: Time::read(&not_after)?,
        public_key: public_key.whole,
        signature_oid: &[],
        signature: &[],
    })
}
