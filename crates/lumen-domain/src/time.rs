//! UTC timestamps with a strict RFC 3339 text form.
//!
//! Filesystem times (modified, accessed, created) and record times (observed,
//! decided, journaled) are UTC instants with nanosecond precision. The text form
//! is canonical RFC 3339 in UTC (`2026-10-07T12:34:56.5Z`), as ADR-0011 requires
//! for everything that crosses a boundary: one instant has exactly one spelling.
//!
//! The domain never reads the clock; "now" comes from a `Clock` port.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

const NANOS_PER_SEC: u32 = 1_000_000_000;
const SECS_PER_DAY: i64 = 86_400;

/// A UTC instant between `0001-01-01T00:00:00Z` and `9999-12-31T23:59:59.999999999Z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    secs: i64,
    nanos: u32,
}

/// Error returned for an out-of-range or malformed timestamp.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TimestampError {
    /// Outside years 0001–9999, or nanoseconds of 10⁹ or more.
    #[error("timestamp is outside 0001-01-01..=9999-12-31 UTC or has invalid nanoseconds")]
    OutOfRange,
    /// Not canonical RFC 3339 UTC (`YYYY-MM-DDTHH:MM:SS[.fraction]Z`).
    #[error("timestamp is not canonical RFC 3339 UTC (YYYY-MM-DDTHH:MM:SS[.fraction]Z)")]
    Malformed,
}

impl Timestamp {
    /// `0001-01-01T00:00:00Z`.
    pub const MIN: Self = Self {
        secs: -62_135_596_800,
        nanos: 0,
    };
    /// `9999-12-31T23:59:59.999999999Z`.
    pub const MAX: Self = Self {
        secs: 253_402_300_799,
        nanos: NANOS_PER_SEC - 1,
    };
    /// `1970-01-01T00:00:00Z`.
    pub const UNIX_EPOCH: Self = Self { secs: 0, nanos: 0 };

    /// Creates a timestamp from seconds and nanoseconds since the Unix epoch.
    ///
    /// # Errors
    ///
    /// Returns [`TimestampError::OutOfRange`] outside the supported range or if
    /// `nanos` is at least one second.
    pub fn from_unix(secs: i64, nanos: u32) -> Result<Self, TimestampError> {
        let ts = Self { secs, nanos };
        if nanos >= NANOS_PER_SEC || ts < Self::MIN || ts > Self::MAX {
            Err(TimestampError::OutOfRange)
        } else {
            Ok(ts)
        }
    }

    /// Whole seconds since the Unix epoch (negative before 1970).
    pub const fn unix_seconds(self) -> i64 {
        self.secs
    }

    /// Nanoseconds past [`Self::unix_seconds`].
    pub const fn subsec_nanos(self) -> u32 {
        self.nanos
    }

    /// Whole days elapsed from `earlier` to `self`, rounded toward zero; negative
    /// if `earlier` is later.
    pub fn whole_days_since(self, earlier: Self) -> i64 {
        let mut secs = self.secs - earlier.secs;
        if secs > 0 && self.nanos < earlier.nanos {
            secs -= 1;
        } else if secs < 0 && self.nanos > earlier.nanos {
            secs += 1;
        }
        secs / SECS_PER_DAY
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant,
/// "chrono-Compatible Low-Level Date Algorithms", `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let days = self.secs.div_euclid(SECS_PER_DAY);
        let secs_of_day = self.secs.rem_euclid(SECS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        let (hour, minute, second) = (secs_of_day / 3600, secs_of_day / 60 % 60, secs_of_day % 60);
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
        )?;
        if self.nanos != 0 {
            let fraction = format!("{:09}", self.nanos);
            write!(f, ".{}", fraction.trim_end_matches('0'))?;
        }
        f.write_str("Z")
    }
}

/// Parses exactly `width` ASCII digits.
fn digits(text: &[u8], width: usize) -> Option<i64> {
    if text.len() != width || !text.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(text.iter().fold(0, |acc, d| acc * 10 + i64::from(d - b'0')))
}

impl FromStr for Timestamp {
    type Err = TimestampError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let malformed = TimestampError::Malformed;
        let b = s.as_bytes();
        if b.len() < 20
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b'T'
            || b[13] != b':'
            || b[16] != b':'
        {
            return Err(malformed);
        }
        let field = |range: std::ops::Range<usize>| {
            digits(&b[range.clone()], range.len()).ok_or(TimestampError::Malformed)
        };
        let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
        let (hour, minute, second) = (field(11..13)?, field(14..16)?, field(17..19)?);
        let rest = &b[19..];
        let nanos = match rest {
            [b'Z'] => 0,
            [b'.', fraction @ .., b'Z'] => {
                // Canonical: 1–9 digits, no trailing zero.
                if fraction.is_empty() || fraction.len() > 9 || fraction.last() == Some(&b'0') {
                    return Err(malformed);
                }
                let value = digits(fraction, fraction.len()).ok_or(TimestampError::Malformed)?;
                let scale = 10_i64
                    .pow(9 - u32::try_from(fraction.len()).map_err(|_| TimestampError::Malformed)?);
                u32::try_from(value * scale).map_err(|_| TimestampError::Malformed)?
            }
            _ => return Err(malformed),
        };
        let valid = year >= 1
            && (1..=12).contains(&month)
            && (1..=days_in_month(year, month)).contains(&day)
            && hour < 24
            && minute < 60
            && second < 60;
        if !valid {
            return Err(malformed);
        }
        let secs =
            days_from_civil(year, month, day) * SECS_PER_DAY + hour * 3600 + minute * 60 + second;
        Self::from_unix(secs, nanos)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn ts(secs: i64, nanos: u32) -> Timestamp {
        Timestamp::from_unix(secs, nanos).unwrap_or_else(|_| unreachable!("in range"))
    }

    #[test]
    fn known_instants_format_canonically() {
        assert_eq!(Timestamp::UNIX_EPOCH.to_string(), "1970-01-01T00:00:00Z");
        assert_eq!(Timestamp::MIN.to_string(), "0001-01-01T00:00:00Z");
        assert_eq!(Timestamp::MAX.to_string(), "9999-12-31T23:59:59.999999999Z");
        assert_eq!(ts(951_782_400, 0).to_string(), "2000-02-29T00:00:00Z");
        assert_eq!(ts(-1, 500_000_000).to_string(), "1969-12-31T23:59:59.5Z");
        assert_eq!(
            ts(1_791_380_096, 120_000).to_string(),
            "2026-10-07T13:34:56.00012Z"
        );
    }

    #[test]
    fn parsing_accepts_only_canonical_utc() -> Result<(), TimestampError> {
        assert_eq!(
            "2000-02-29T00:00:00Z".parse::<Timestamp>()?,
            ts(951_782_400, 0)
        );
        assert_eq!(
            "1969-12-31T23:59:59.5Z".parse::<Timestamp>()?,
            ts(-1, 500_000_000)
        );
        for bad in [
            "2026-10-07T12:34:56",             // no zone
            "2026-10-07T12:34:56+00:00",       // offsets are not canonical
            "2026-10-07t12:34:56z",            // lowercase
            "2026-10-07 12:34:56Z",            // space separator
            "2026-10-07T12:34:56.Z",           // empty fraction
            "2026-10-07T12:34:56.50Z",         // trailing zero
            "2026-10-07T12:34:56.1234567891Z", // 10 digits
            "2026-02-29T00:00:00Z",            // not a leap year
            "1900-02-29T00:00:00Z",            // century rule
            "2026-13-01T00:00:00Z",
            "2026-10-07T24:00:00Z",
            "2026-10-07T12:60:00Z",
            "2026-10-07T12:34:60Z", // leap seconds are not representable
            "0000-01-01T00:00:00Z",
            "+2026-10-07T12:34:56Z",
        ] {
            assert!(bad.parse::<Timestamp>().is_err(), "{bad}");
        }
        assert_eq!(
            "2000-02-29T00:00:00Z".parse::<Timestamp>()?.to_string(),
            "2000-02-29T00:00:00Z"
        );
        Ok(())
    }

    #[test]
    fn range_is_enforced() {
        assert_eq!(
            Timestamp::from_unix(Timestamp::MAX.unix_seconds() + 1, 0),
            Err(TimestampError::OutOfRange)
        );
        assert_eq!(
            Timestamp::from_unix(Timestamp::MIN.unix_seconds() - 1, 0),
            Err(TimestampError::OutOfRange)
        );
        assert_eq!(
            Timestamp::from_unix(0, NANOS_PER_SEC),
            Err(TimestampError::OutOfRange)
        );
    }

    #[test]
    fn whole_days_round_toward_zero() {
        let start = ts(0, 0);
        assert_eq!(ts(SECS_PER_DAY - 1, 999_999_999).whole_days_since(start), 0);
        assert_eq!(ts(SECS_PER_DAY, 0).whole_days_since(start), 1);
        assert_eq!(ts(SECS_PER_DAY, 0).whole_days_since(ts(0, 1)), 0);
        assert_eq!(start.whole_days_since(ts(SECS_PER_DAY, 0)), -1);
    }

    #[test]
    fn serializes_as_rfc3339_string() -> serde_json::Result<()> {
        assert_eq!(
            serde_json::to_string(&ts(0, 0))?,
            "\"1970-01-01T00:00:00Z\""
        );
        assert!(serde_json::from_str::<Timestamp>("0").is_err());
        Ok(())
    }

    fn arb_timestamp() -> impl Strategy<Value = Timestamp> {
        (
            Timestamp::MIN.unix_seconds()..=Timestamp::MAX.unix_seconds(),
            0..NANOS_PER_SEC,
        )
            .prop_map(|(secs, nanos)| ts(secs, nanos))
    }

    proptest! {
        #[test]
        fn text_round_trips(t in arb_timestamp()) {
            prop_assert_eq!(t.to_string().parse::<Timestamp>(), Ok(t));
        }

        #[test]
        fn day_differences_are_antisymmetric(a in arb_timestamp(), b in arb_timestamp()) {
            prop_assert_eq!(a.whole_days_since(b), -b.whole_days_since(a));
        }

        #[test]
        fn civil_conversion_round_trips(days in -719_162i64..2_932_896) {
            let (y, m, d) = civil_from_days(days);
            prop_assert_eq!(days_from_civil(y, m, d), days);
            prop_assert!((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m));
        }

        #[test]
        fn parser_never_panics(s in "\\PC{0,40}") {
            let _parsed = s.parse::<Timestamp>();
        }
    }
}
