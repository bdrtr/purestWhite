//! A certificate's dates, read as mbedTLS 3.6 reads them (`mbedtls_x509_get_time`), which the
//! C++ firmware's bundle check uses: `UTCTime` or `GeneralizedTime`, seconds always given, a
//! trailing `Z` optional, a two-digit year below 50 in the 2000s.

use crate::der::{Element, GENERALIZED_TIME, UTC_TIME};

/// A moment, in UTC, by its calendar fields; later moments compare greater.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Time {
    /// `seconds` since 1970-01-01 00:00 UTC.
    #[must_use]
    pub fn from_unix(seconds: u64) -> Time {
        let days = seconds / 86_400;
        let of_day = seconds % 86_400;
        let (year, month, day) = civil_from_days(days);
        let field = |v: u64| u8::try_from(v).unwrap_or(u8::MAX);
        Time {
            year,
            month,
            day,
            hour: field(of_day / 3600),
            minute: field(of_day / 60 % 60),
            second: field(of_day % 60),
        }
    }

    /// The time `element` gives; none when it is not one mbedTLS reads.
    pub(crate) fn read(element: &Element<'_>) -> Option<Time> {
        let year_len = match element.tag {
            UTC_TIME => 2,
            GENERALIZED_TIME => 4,
            _ => return None,
        };
        let text = element.content;
        let plain = text.len() == year_len + 10;
        let zoned = text.len() == year_len + 11 && text.last() == Some(&b'Z');
        if !plain && !zoned {
            return None;
        }
        let (year, rest) = text.split_at_checked(year_len)?;
        let year = match year {
            [a, b] => {
                let two = two_digits(*a, *b)?;
                u16::from(two) + if two < 50 { 2000 } else { 1900 }
            }
            [a, b, c, d] => u16::from(two_digits(*a, *b)?) * 100 + u16::from(two_digits(*c, *d)?),
            _ => return None,
        };
        let mut fields = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[a, b]| two_digits(*a, *b));
        let mut take = || fields.next().flatten();
        let time = Time {
            year,
            month: take()?,
            day: take()?,
            hour: take()?,
            minute: take()?,
            second: take()?,
        };
        time.is_valid().then_some(time)
    }

    /// As mbedTLS's `x509_date_is_valid`.
    fn is_valid(self) -> bool {
        let y = self.year;
        let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
        let month_days = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return false,
        };
        (1..=month_days).contains(&self.day)
            && self.year <= 9999
            && self.hour <= 23
            && self.minute <= 59
            && self.second <= 59
    }
}

fn two_digits(a: u8, b: u8) -> Option<u8> {
    let digit = |c: u8| c.checked_sub(b'0').filter(|d| *d < 10);
    Some(digit(a)? * 10 + digit(b)?)
}

/// The civil date of `days` since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: u64) -> (u16, u8, u8) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    let narrow = |v: u64| u8::try_from(v).unwrap_or(u8::MAX);
    (
        u16::try_from(year).unwrap_or(u16::MAX),
        narrow(month),
        narrow(day),
    )
}
