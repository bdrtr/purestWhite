//! ArduinoJson's float arithmetic (`Numbers/FloatTraits.hpp`, `FloatParts.hpp`): decimal numbers
//! made by multiplying by its tables of binary powers of ten, and floats taken apart into the
//! digits it writes. Not correctly rounded; the same bits as ArduinoJson's on any IEEE machine.

/// 1e1, 1e2, 1e4, ... 1e256, and their inverses, as ArduinoJson's tables give them.
const POSITIVE_64: [u64; 9] = [
    0x4024_0000_0000_0000,
    0x4059_0000_0000_0000,
    0x40C3_8800_0000_0000,
    0x4197_D784_0000_0000,
    0x4341_C379_37E0_8000,
    0x4693_B8B5_B505_6E17,
    0x4D38_4F03_E93F_F9F5,
    0x5A82_7748_F930_1D32,
    0x7515_4FDD_7F73_BF3C,
];
const NEGATIVE_64: [u64; 9] = [
    0x3FB9_9999_9999_999A,
    0x3F84_7AE1_47AE_147B,
    0x3F1A_36E2_EB1C_432D,
    0x3E45_798E_E230_8C3A,
    0x3C9C_D2B2_97D8_89BC,
    0x3949_F623_D5A8_A733,
    0x32A5_0FFD_44F4_A73D,
    0x255B_BA08_CF8C_979D,
    0x0AC8_0628_64AC_6F43,
];
/// 1e1f, 1e2f, 1e4f, ... 1e32f, and their inverses.
const POSITIVE_32: [u32; 6] = [
    0x4120_0000,
    0x42c8_0000,
    0x461c_4000,
    0x4cbe_bc20,
    0x5a0e_1bca,
    0x749d_c5ae,
];
const NEGATIVE_32: [u32; 6] = [
    0x3dcc_cccd,
    0x3c23_d70a,
    0x38d1_b717,
    0x322b_cc77,
    0x24e6_9595,
    0x0a4f_b11f,
];

/// Writing switches to an exponent from 1e7 up, and from 1e-5 down.
const POSITIVE_THRESHOLD: f64 = 1e7;
const NEGATIVE_THRESHOLD: f64 = 1e-5;

/// `m` × 10^`e`, as ArduinoJson's `make_float<double>`. An exponent past its tables (only a
/// number string far longer than JSON's 63-byte token can have one; ArduinoJson would read past
/// the table there) multiplies by nothing more.
pub(crate) fn make_f64(mut m: f64, e: i32) -> f64 {
    let table = if e > 0 { &POSITIVE_64 } else { &NEGATIVE_64 };
    let mut e = e.unsigned_abs();
    for factor in table {
        if e == 0 {
            break;
        }
        if e & 1 != 0 {
            m *= f64::from_bits(*factor);
        }
        e >>= 1;
    }
    m
}

/// `m` × 10^`e`, as ArduinoJson's `make_float<float>` (`e` within ±38 here).
pub(crate) fn make_f32(mut m: f32, e: i32) -> f32 {
    let table = if e > 0 { &POSITIVE_32 } else { &NEGATIVE_32 };
    let mut e = e.unsigned_abs();
    for factor in table {
        if e == 0 {
            break;
        }
        if e & 1 != 0 {
            m *= f32::from_bits(*factor);
        }
        e >>= 1;
    }
    m
}

/// A float as ArduinoJson writes it: integral digits, decimal digits (`decimal_places` of them,
/// leading zeros kept) and a power of ten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Parts {
    pub integral: u32,
    pub decimal: u32,
    pub exponent: i16,
    pub decimal_places: u32,
}

/// `value` (positive, finite) brought to [1, 10) when it is past the thresholds: the power of ten
/// taken out (`normalize`).
fn normalize(value: &mut f64) -> i16 {
    let mut powers: i16 = 0;
    // Both passes share the index, as in ArduinoJson: once the first ran, the second does not.
    let mut index: i32 = 8;
    let mut bit: i16 = 1 << 8;
    let factor = |table: &[u64; 9], at: i32| {
        usize::try_from(at)
            .ok()
            .and_then(|at| table.get(at))
            .map_or(1.0, |b| f64::from_bits(*b))
    };
    if *value >= POSITIVE_THRESHOLD {
        while index >= 0 {
            if *value >= factor(&POSITIVE_64, index) {
                *value *= factor(&NEGATIVE_64, index);
                powers = powers.wrapping_add(bit);
            }
            bit >>= 1;
            index -= 1;
        }
    }
    if *value > 0.0 && *value <= NEGATIVE_THRESHOLD {
        while index >= 0 {
            if *value < factor(&NEGATIVE_64, index) * 10.0 {
                *value *= factor(&POSITIVE_64, index);
                powers = powers.wrapping_sub(bit);
            }
            bit >>= 1;
            index -= 1;
        }
    }
    powers
}

/// `value` (positive, finite) taken apart for `decimal_places` (6 for a `float`, 9 for a
/// `double`), as ArduinoJson's `decomposeFloat`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "ArduinoJson's own truncating conversions, kept"
)]
pub(crate) fn decompose(mut value: f64, decimal_places: u32) -> Parts {
    let mut max_decimal = 10u32.pow(decimal_places);
    let mut places = decimal_places;
    let mut exponent = normalize(&mut value);
    let mut integral = value as u32;
    let mut tmp = integral;
    while tmp >= 10 {
        max_decimal /= 10;
        places = places.saturating_sub(1);
        tmp /= 10;
    }
    let mut remainder = (value - f64::from(integral)) * f64::from(max_decimal);
    let mut decimal = remainder as u32;
    remainder -= f64::from(decimal);
    decimal = decimal.wrapping_add((remainder * 2.0) as u32);
    if decimal >= max_decimal {
        decimal = 0;
        integral = integral.wrapping_add(1);
        if exponent != 0 && integral >= 10 {
            exponent = exponent.wrapping_add(1);
            integral = 1;
        }
    }
    while decimal.is_multiple_of(10) && places > 0 {
        decimal /= 10;
        places -= 1;
    }
    Parts {
        integral,
        decimal,
        exponent,
        decimal_places: places,
    }
}
