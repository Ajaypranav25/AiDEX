use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    MAX_UNIX_MILLIS, MIN_UNIX_MILLIS, Timestamp, TimestampError, civil_from_days, days_from_civil,
};

fn ts(value: &str) -> Timestamp {
    Timestamp::new(value).unwrap()
}

#[test]
fn civil_conversion_round_trips_known_dates_and_leap_days() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    assert_eq!(days_from_civil(1969, 12, 31), -1);
    assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    for days in (-800_000..=3_000_000).step_by(997) {
        let (year, month, day) = civil_from_days(days);
        assert_eq!(days_from_civil(year, month, day), days);
    }
}

#[test]
fn canonical_millisecond_formatting_and_parsing_agree() {
    let epoch = Timestamp::from_unix_millis(0).unwrap();
    assert_eq!(epoch.as_str(), "1970-01-01T00:00:00.000Z");
    assert_eq!(epoch.unix_millis(), 0);

    let sample = Timestamp::from_unix_millis(1_791_618_661_042).unwrap();
    assert_eq!(sample.as_str(), "2026-10-10T07:51:01.042Z");
    assert_eq!(ts(sample.as_str()), sample);

    let before_epoch = Timestamp::from_unix_millis(-1).unwrap();
    assert_eq!(before_epoch.as_str(), "1969-12-31T23:59:59.999Z");
    assert_eq!(before_epoch.unix_millis(), -1);

    let leap = Timestamp::from_unix_millis(1_709_164_800_000).unwrap();
    assert_eq!(leap.as_str(), "2024-02-29T00:00:00.000Z");
}

#[test]
fn unix_millis_parses_every_accepted_fractional_form() {
    assert_eq!(ts("2026-10-10T07:51:01Z").unix_millis(), 1_791_618_661_000);
    assert_eq!(
        ts("2026-10-10T07:51:01.5Z").unix_millis(),
        1_791_618_661_500
    );
    assert_eq!(
        ts("2026-10-10T07:51:01.04Z").unix_millis(),
        1_791_618_661_040
    );
    assert_eq!(
        ts("2026-10-10T07:51:01.042999999Z").unix_millis(),
        1_791_618_661_042
    );
    assert_eq!(
        ts("1969-12-31T23:59:59.9999Z").unix_millis(),
        -1,
        "sub-millisecond precision truncates toward the past"
    );
    assert_eq!(ts("0000-01-01T00:00:00Z").unix_millis(), MIN_UNIX_MILLIS);
    assert_eq!(
        ts("9999-12-31T23:59:59.999Z").unix_millis(),
        MAX_UNIX_MILLIS
    );
}

#[test]
fn ordering_is_chronological_across_precisions() {
    let whole = ts("2026-10-10T07:51:01Z");
    let half = ts("2026-10-10T07:51:01.5Z");
    let later = ts("2026-10-10T07:51:02Z");
    assert!(whole < half, "lexical order would put `.5Z` first");
    assert!(half < later);
    assert!(ts("2026-10-10T07:51:01.123456789Z") > ts("2026-10-10T07:51:01.123Z"));

    let same_instant = ts("2026-10-10T07:51:01.000Z");
    assert_ne!(whole, same_instant);
    assert_ne!(whole.cmp(&same_instant), std::cmp::Ordering::Equal);
    assert_eq!(whole.unix_millis(), same_instant.unix_millis());

    let mut values = vec![later.clone(), half.clone(), whole.clone()];
    values.sort();
    assert_eq!(values, vec![whole, half, later]);
}

#[test]
fn range_limits_are_enforced() {
    assert!(Timestamp::from_unix_millis(MIN_UNIX_MILLIS).is_ok());
    assert!(Timestamp::from_unix_millis(MAX_UNIX_MILLIS).is_ok());
    assert_eq!(
        Timestamp::from_unix_millis(MIN_UNIX_MILLIS - 1),
        Err(TimestampError)
    );
    assert_eq!(
        Timestamp::from_unix_millis(MAX_UNIX_MILLIS + 1),
        Err(TimestampError)
    );
    assert_eq!(Timestamp::from_unix_millis(i64::MAX), Err(TimestampError));
    assert_eq!(Timestamp::from_unix_millis(i64::MIN), Err(TimestampError));
}

#[test]
fn system_time_conversion_truncates_toward_the_past() {
    let after = UNIX_EPOCH + Duration::from_micros(1_500);
    assert_eq!(
        Timestamp::from_system_time(after).unwrap().as_str(),
        "1970-01-01T00:00:00.001Z"
    );
    let before = UNIX_EPOCH - Duration::from_micros(500);
    assert_eq!(
        Timestamp::from_system_time(before).unwrap().as_str(),
        "1969-12-31T23:59:59.999Z"
    );
}

#[test]
fn now_is_canonical_and_tracks_the_system_clock() {
    let lower = Timestamp::from_system_time(SystemTime::now()).unwrap();
    let now = Timestamp::now();
    let upper = Timestamp::from_system_time(SystemTime::now()).unwrap();
    assert!(lower <= now && now <= upper);
    let text = now.as_str();
    assert_eq!(text.len(), 24);
    assert_eq!(&text[19..20], ".");
    assert!(text.ends_with('Z'));
    assert_eq!(ts(text), now);
}

#[test]
fn malformed_timestamps_are_rejected() {
    for value in [
        "",
        "2026-10-10",
        "2026-10-10T07:51:01",
        "2026-10-10T07:51:01.Z",
        "2026-10-10T07:51:01.1234567890Z",
        "2026-10-10T07:51:60Z",
        "2026-10-10T24:00:00Z",
        "2026-13-10T07:51:01Z",
        "2023-02-29T00:00:00Z",
        "2026-10-10T07:51:01+00:00",
        "2026-10-10 07:51:01Z",
        "2026-10-10T07:51:01.1a3Z",
    ] {
        assert_eq!(Timestamp::new(value), Err(TimestampError), "{value:?}");
    }
}
