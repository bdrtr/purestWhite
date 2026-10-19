//! Numbers as C++ `std::from_chars` reads them, which is what the stylesheets were read with.
//!
//! A float is `-`? then `inf`, `infinity`, `nan`, `nan(...)` or decimal digits with an optional
//! point and exponent, rounded to the nearest `f32` (ties to even). A value that rounds to
//! infinity or, from non-zero digits, to zero is out of range: no number. The whole text must be
//! the number.

/// The `f32` that `s` names, or `None`.
pub(crate) fn parse_f32(s: &[u8]) -> Option<f32> {
    let (negative, s) = match s {
        [b'-', rest @ ..] => (true, rest),
        _ => (false, s),
    };
    let magnitude = special(s).or_else(|| decimal(s))?;
    Some(if negative { -magnitude } else { magnitude })
}

/// The integer that `s` names, `-`? then digits, if it fits 32 bits (a `long` on the device).
pub(crate) fn parse_i32(s: &[u8]) -> Option<i32> {
    let (negative, digits) = match s {
        [b'-', rest @ ..] => (true, rest),
        _ => (false, s),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value = 0i64;
    for &b in digits {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i64::from(b - b'0');
        if value > 1 << 31 {
            return None;
        }
    }
    i32::try_from(if negative { -value } else { value }).ok()
}

fn special(s: &[u8]) -> Option<f32> {
    if s.eq_ignore_ascii_case(b"inf") || s.eq_ignore_ascii_case(b"infinity") {
        return Some(f32::INFINITY);
    }
    let (head, rest) = s.split_at_checked(3)?;
    if !head.eq_ignore_ascii_case(b"nan") {
        return None;
    }
    let nan = match rest {
        [] => true,
        [b'(', inner @ .., b')'] => inner
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_'),
        _ => false,
    };
    nan.then_some(f32::NAN)
}

/// Significant digits kept. Every point halfway between two `f32` values has at most 113
/// significant digits, so digits past these only tell whether the value is exactly such a
/// point: one sticky bit carries that.
const MAX_DIGITS: usize = 120;

/// `0.d1d2...dn × 10^point`, the digits without leading or trailing zeros.
struct Decimal {
    digits: [u8; MAX_DIGITS],
    len: usize,
    point: i32,
    /// Non-zero digits were dropped past [`MAX_DIGITS`].
    sticky: bool,
}

impl Decimal {
    /// Reads `digits[.digits][e[+-]digits]`; `None` if anything else is there.
    fn read(s: &[u8]) -> Option<Self> {
        let mut decimal = Decimal {
            digits: [0; MAX_DIGITS],
            len: 0,
            point: 0,
            sticky: false,
        };
        let mut seen_digit = false;
        let mut seen_point = false;
        let mut rest = s;
        while let [b, tail @ ..] = rest {
            match b {
                b'0'..=b'9' => {
                    seen_digit = true;
                    decimal.push(b - b'0', seen_point);
                }
                b'.' if !seen_point => seen_point = true,
                _ => break,
            }
            rest = tail;
        }
        if !seen_digit {
            return None;
        }
        if let [b'e' | b'E', exponent @ ..] = rest {
            decimal.point = decimal.point.saturating_add(exponent_value(exponent)?);
        } else if !rest.is_empty() {
            return None;
        }
        while decimal.len > 0 && decimal.digits.get(decimal.len - 1) == Some(&0) {
            decimal.len -= 1;
        }
        Some(decimal)
    }

    fn push(&mut self, digit: u8, after_point: bool) {
        if digit == 0 && self.len == 0 {
            // A leading zero: only one after the point moves the value.
            if after_point {
                self.point -= 1;
            }
            return;
        }
        if let Some(slot) = self.digits.get_mut(self.len) {
            *slot = digit;
            self.len += 1;
        } else if digit != 0 {
            self.sticky = true;
        }
        if !after_point {
            self.point = self.point.saturating_add(1);
        }
    }

    fn significant(&self) -> &[u8] {
        self.digits.get(..self.len).unwrap_or(&[])
    }
}

/// `[+-]digits`, saturating far past any `f32` exponent.
fn exponent_value(s: &[u8]) -> Option<i32> {
    let (negative, digits) = match s {
        [b'-', rest @ ..] => (true, rest),
        [b'+', rest @ ..] => (false, rest),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let value = digits
        .iter()
        .fold(0i32, |v, &b| (v * 10 + i32::from(b - b'0')).min(1 << 20));
    Some(if negative { -value } else { value })
}

fn decimal(s: &[u8]) -> Option<f32> {
    let d = Decimal::read(s)?;
    if d.len == 0 {
        return Some(0.0);
    }
    // At least 10^38.99 ≥ 2^128 rounds to infinity; below 10^-46 < 2^-150, to zero.
    if d.point > 39 || d.point < -45 {
        return None;
    }
    let exponent = d.point - i32::try_from(d.len).ok()?;
    fast(d.significant(), exponent).or_else(|| exact(&d, exponent))
}

/// Up to 2^24 times a power of ten that `f32` holds: one correctly rounded operation.
fn fast(digits: &[u8], exponent: i32) -> Option<f32> {
    const POW10: [f32; 11] = [1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10];
    if digits.len() > 8 {
        return None;
    }
    let value = digits.iter().fold(0u32, |v, &d| v * 10 + u32::from(d));
    if value > 1 << 24 {
        return None;
    }
    #[allow(clippy::cast_precision_loss, reason = "value <= 2^24 is exact")]
    let value = value as f32;
    let scale = POW10.get(usize::try_from(exponent.unsigned_abs()).ok()?)?;
    Some(if exponent < 0 {
        value / scale
    } else {
        value * scale
    })
}

/// `digits × 10^exponent` rounded through exact integers: `q = floor(value × 2^s)` holds the
/// 24 bits of the result and one more, the remainder says whether anything is below them.
fn exact(d: &Decimal, exponent: i32) -> Option<f32> {
    let mut num = Big::from_digits(d.significant())?;
    let mut den = Big::one();
    if exponent >= 0 {
        num.mul_pow10(exponent.unsigned_abs())?;
    } else {
        den.mul_pow10(exponent.unsigned_abs())?;
    }
    // value is in (2^(l-1), 2^(l+1)).
    let l = i32::try_from(num.bit_len()).ok()? - i32::try_from(den.bit_len()).ok()?;
    // Below 2^-126 the scale stays at the subnormal one, 2^150.
    let mut s = (25 - l).min(150);
    let (mut q, mut rest) = scaled_quotient(&num, &den, s)?;
    if q >= 1 << 25 {
        s -= 1;
        (q, rest) = scaled_quotient(&num, &den, s)?;
    }
    let round = q & 1 == 1;
    let mut mantissa = q >> 1;
    if round && (rest || d.sticky || mantissa & 1 == 1) {
        mantissa += 1;
    }
    if mantissa == 0 {
        return None;
    }
    // The mantissa's leading bit lands in the exponent field, so a carry out of it (and a
    // subnormal that rounds up to 2^-126) moves the exponent by itself.
    let bits = u32::try_from(150 - s)
        .ok()?
        .checked_shl(23)?
        .checked_add(mantissa)?;
    (bits < 0x7F80_0000).then(|| f32::from_bits(bits))
}

/// `floor(num / den × 2^s)`, which must be below 2^26, and whether it left a remainder.
fn scaled_quotient(num: &Big, den: &Big, s: i32) -> Option<(u32, bool)> {
    let mut num = num.clone();
    let mut den = den.clone();
    if s >= 0 {
        num.shl(s.unsigned_abs())?;
    } else {
        den.shl(s.unsigned_abs())?;
    }
    den.shl(25)?;
    let mut q = 0;
    for bit in (0..=25).rev() {
        if num >= den {
            num.sub(&den);
            q |= 1 << bit;
        }
        den.shr1();
    }
    Some((q, !num.is_zero()))
}

/// An unsigned integer of up to 640 bits, enough for 120 digits scaled to the
/// smallest `f32` (2^150) and 10^166 shifted by 25 bits.
#[derive(Clone, PartialEq, Eq)]
struct Big {
    /// Least significant first; the limbs past `len` are zero.
    limbs: [u32; 20],
    len: usize,
}

impl Big {
    fn one() -> Self {
        Big {
            limbs: core::array::from_fn(|i| u32::from(i == 0)),
            len: 1,
        }
    }

    fn from_digits(digits: &[u8]) -> Option<Self> {
        let mut big = Big {
            limbs: [0; 20],
            len: 0,
        };
        for chunk in digits.chunks(9) {
            let mut scale = 1;
            let mut value = 0;
            for &d in chunk {
                scale *= 10;
                value = value * 10 + u32::from(d);
            }
            big.mul_add(scale, value)?;
        }
        Some(big)
    }

    fn is_zero(&self) -> bool {
        self.len == 0
    }

    fn bit_len(&self) -> usize {
        self.len.checked_sub(1).map_or(0, |top| {
            top * 32 + 32
                - self
                    .limbs
                    .get(top)
                    .map_or(32, |l| l.leading_zeros() as usize)
        })
    }

    /// `self × factor + add`.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the low and high halves of a u64"
    )]
    fn mul_add(&mut self, factor: u32, add: u32) -> Option<()> {
        let mut carry = u64::from(add);
        for limb in self.limbs.iter_mut().take(self.len) {
            let v = u64::from(*limb) * u64::from(factor) + carry;
            *limb = v as u32;
            carry = v >> 32;
        }
        if carry != 0 {
            *self.limbs.get_mut(self.len)? = carry as u32;
            self.len += 1;
        }
        Some(())
    }

    fn mul_pow10(&mut self, mut exponent: u32) -> Option<()> {
        while exponent > 0 {
            let step = exponent.min(9);
            self.mul_add(10u32.pow(step), 0)?;
            exponent -= step;
        }
        Some(())
    }

    fn shl(&mut self, bits: u32) -> Option<()> {
        if self.is_zero() {
            return Some(());
        }
        let limbs = (bits / 32) as usize;
        let bits = bits % 32;
        let len = self.len + limbs + 1;
        if len > self.limbs.len() + 1 || (len > self.limbs.len() && self.top_bits(bits) != 0) {
            return None;
        }
        for i in (0..len.min(self.limbs.len())).rev() {
            let high = i
                .checked_sub(limbs)
                .and_then(|j| self.limbs.get(j))
                .copied()
                .unwrap_or(0);
            let low = i
                .checked_sub(limbs + 1)
                .and_then(|j| self.limbs.get(j))
                .copied()
                .unwrap_or(0);
            let v = if bits == 0 {
                high
            } else {
                high << bits | low >> (32 - bits)
            };
            if let Some(limb) = self.limbs.get_mut(i) {
                *limb = v;
            }
        }
        self.len = len.min(self.limbs.len());
        self.trim();
        Some(())
    }

    /// The bits a shift by `bits` (< 32) moves out of the top limb.
    fn top_bits(&self, bits: u32) -> u32 {
        let top = self
            .len
            .checked_sub(1)
            .and_then(|i| self.limbs.get(i))
            .copied()
            .unwrap_or(0);
        if bits == 0 { 0 } else { top >> (32 - bits) }
    }

    fn shr1(&mut self) {
        let mut carry = 0;
        for limb in self.limbs.iter_mut().take(self.len).rev() {
            let v = *limb;
            *limb = v >> 1 | carry << 31;
            carry = v & 1;
        }
        self.trim();
    }

    /// `self - other`, which must not be negative.
    fn sub(&mut self, other: &Big) {
        let mut borrow = 0;
        for (i, limb) in self.limbs.iter_mut().enumerate().take(self.len) {
            let o = other.limbs.get(i).copied().unwrap_or(0);
            let (v, b1) = limb.overflowing_sub(o);
            let (v, b2) = v.overflowing_sub(borrow);
            *limb = v;
            borrow = u32::from(b1 || b2);
        }
        self.trim();
    }

    fn trim(&mut self) {
        while self.len > 0 && self.limbs.get(self.len - 1) == Some(&0) {
            self.len -= 1;
        }
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Big {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.len.cmp(&other.len).then_with(|| {
            let a = self.limbs.iter().take(self.len).rev();
            a.cmp(other.limbs.iter().take(other.len).rev())
        })
    }
}
