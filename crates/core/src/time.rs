//! Validated UTC timestamps and a dependency-free civil calendar conversion.

use std::{
    cmp::Ordering,
    error::Error,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

const MILLIS_PER_DAY: i64 = 86_400_000;
const SECONDS_PER_DAY: i64 = 86_400;
/// `0000-01-01T00:00:00.000Z` in Unix milliseconds.
const MIN_UNIX_MILLIS: i64 = days_from_civil(0, 1, 1) * MILLIS_PER_DAY;
/// `9999-12-31T23:59:59.999Z` in Unix milliseconds.
const MAX_UNIX_MILLIS: i64 = days_from_civil(9999, 12, 31) * MILLIS_PER_DAY + MILLIS_PER_DAY - 1;

/// Validated RFC 3339 UTC timestamp, serialized canonically with a trailing `Z`.
///
/// Ordering is chronological (fractional seconds of any accepted precision compare
/// by instant); spellings of the same instant are tie-broken by their text so that
/// ordering stays consistent with equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timestamp {
    text: String,
    /// Whole seconds since the Unix epoch (floor).
    seconds: i64,
    /// Nanoseconds within `seconds`, `0..1_000_000_000`.
    nanos: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimestampError;

impl fmt::Display for TimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("timestamp must be a valid RFC 3339 UTC timestamp ending in Z")
    }
}
impl Error for TimestampError {}

impl Timestamp {
    /// Accept `YYYY-MM-DDTHH:MM:SSZ` or `YYYY-MM-DDTHH:MM:SS.f{1,9}Z` (years 0000-9999).
    pub fn new(value: impl Into<String>) -> Result<Self, TimestampError> {
        let text = value.into();
        let (seconds, nanos) = parse_timestamp(&text).ok_or(TimestampError)?;
        Ok(Self {
            text,
            seconds,
            nanos,
        })
    }

    /// The current UTC time at millisecond precision, e.g. `2026-10-10T07:51:01.042Z`.
    ///
    /// A system clock outside years 0000-9999 saturates to the nearest representable
    /// instant rather than panicking.
    pub fn now() -> Self {
        let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(MAX_UNIX_MILLIS),
            Err(before) => i64::try_from(before.duration().as_millis())
                .map_or(MIN_UNIX_MILLIS, |millis| -millis),
        };
        Self::from_clamped_millis(millis.clamp(MIN_UNIX_MILLIS, MAX_UNIX_MILLIS))
    }

    /// Convert a system time, truncating toward the past to millisecond precision.
    pub fn from_system_time(time: SystemTime) -> Result<Self, TimestampError> {
        let millis = match time.duration_since(UNIX_EPOCH) {
            Ok(elapsed) => i64::try_from(elapsed.as_millis()).map_err(|_| TimestampError)?,
            Err(before) => {
                let before = before.duration();
                let partial = u128::from(before.subsec_nanos() % 1_000_000 != 0);
                let millis =
                    i64::try_from(before.as_millis() + partial).map_err(|_| TimestampError)?;
                -millis
            }
        };
        Self::from_unix_millis(millis)
    }

    /// Canonical `YYYY-MM-DDTHH:MM:SS.mmmZ` for a Unix-millisecond instant.
    pub fn from_unix_millis(millis: i64) -> Result<Self, TimestampError> {
        if !(MIN_UNIX_MILLIS..=MAX_UNIX_MILLIS).contains(&millis) {
            return Err(TimestampError);
        }
        Ok(Self::from_clamped_millis(millis))
    }

    /// Milliseconds since the Unix epoch, truncating sub-millisecond precision toward the past.
    pub fn unix_millis(&self) -> i64 {
        self.seconds * 1_000 + i64::from(self.nanos / 1_000_000)
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Format an in-range instant; callers guarantee the range.
    fn from_clamped_millis(millis: i64) -> Self {
        let days = millis.div_euclid(MILLIS_PER_DAY);
        let within = millis.rem_euclid(MILLIS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        let text = format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
            within / 3_600_000,
            within / 60_000 % 60,
            within / 1_000 % 60,
            within % 1_000,
        );
        Self {
            text,
            seconds: millis.div_euclid(1_000),
            nanos: u32::try_from(millis.rem_euclid(1_000) * 1_000_000).unwrap_or(0),
        }
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.seconds, self.nanos, &self.text).cmp(&(other.seconds, other.nanos, &other.text))
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Parse a validated timestamp into (floor seconds since epoch, nanoseconds).
fn parse_timestamp(s: &str) -> Option<(i64, u32)> {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || *b.last()? != b'Z'
        || ![0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
    {
        return None;
    }
    let num = |start: usize, end: usize| s.get(start..end)?.parse::<u32>().ok();
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let nanos = if b.len() == 20 {
        0
    } else {
        if b[19] != b'.' || b.len() < 22 || b.len() > 30 {
            return None;
        }
        let fraction = &b[20..b.len() - 1];
        if !fraction.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let digits = fraction
            .iter()
            .fold(0_u32, |value, digit| value * 10 + u32::from(digit - b'0'));
        digits * 10_u32.pow(9 - u32::try_from(fraction.len()).ok()?)
    };
    let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    let seconds = days * SECONDS_PER_DAY
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second);
    Some((seconds, nanos))
}

fn days_in_month(year: u32, month: u32) -> u32 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Proleptic Gregorian (year, month, day) for days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text)
    }
}
impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(de::Error::custom)
    }
}
impl JsonSchema for Timestamp {
    fn schema_name() -> String {
        "Timestamp".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
