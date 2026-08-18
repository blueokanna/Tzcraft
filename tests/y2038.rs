//! Y2038 (and beyond) regression suite.
//!
//! The Year 2038 problem is a 32-bit signed-seconds overflow:
//! `1970-01-01` + `i32::MAX` seconds = `2038-01-19T03:14:07Z`, and one second
//! later the counter would wrap negative. `tzcraft` cannot hit this by
//! construction:
//!
//! - [`Ticks`] is `i128` nanoseconds since the epoch (range ≈ ±292 billion
//!   years);
//! - every `timestamp*` accessor and `from_timestamp*` constructor uses
//!   `i64` seconds (range ≈ ±292 billion years, i.e. `i64::MAX` seconds);
//! - [`Date`] is `i32` **days** since the epoch (range ≈ ±5.8 million
//!   years), never seconds;
//! - the only `i32` time-domain value, [`Offset`], is bounded to ±24 hours
//!   and validated at construction.
//!
//! These tests pin the boundary behaviours so a future refactor cannot
//! silently reintroduce a 32-bit second somewhere.

use tzcraft::{Duration, Ticks};

#[test]
fn y2038_boundary_seconds() {
    // 2038-01-19T03:14:07Z == i32::MAX seconds — the classic rollover point.
    let last = Ticks::from_timestamp(2_147_483_647, 999_999_999).unwrap();
    assert_eq!(last.timestamp().unwrap(), 2_147_483_647);
    assert_eq!(last.timestamp_nanos().unwrap(), 2_147_483_647_999_999_999);

    // One nanosecond later must keep working (i64 seconds, not i32).
    let next = last.checked_add(Duration::from_nanos(1)).unwrap();
    assert_eq!(next.timestamp().unwrap(), 2_147_483_648);
    let (y, m, d) = next.to_civil_utc().unwrap().date().parts();
    assert_eq!((y, m, d), (2038, 1, 19));

    // The first overflowing second parses from text and round-trips.
    let s = Ticks::from_rfc3339("2038-01-19T03:14:08Z").unwrap();
    assert_eq!(s.timestamp().unwrap(), 2_147_483_648);
    assert_eq!(s.timestamp_millis().unwrap(), 2_147_483_648_000);
}

#[test]
fn pre_epoch_extremes_floor() {
    // i32::MIN seconds = 1901-12-13T20:45:52Z (the opposite extreme).
    let t = Ticks::from_timestamp(-2_147_483_648, 0).unwrap();
    let (y, m, d) = t.to_civil_utc().unwrap().date().parts();
    assert_eq!((y, m, d), (1901, 12, 13));

    // Floor semantics hold for pre-epoch fractional instants: -0.5 s -> -1.
    let before = Ticks::EPOCH
        .checked_sub(Duration::from_millis(500))
        .unwrap();
    assert_eq!(before.timestamp().unwrap(), -1);
    assert_eq!(before.to_unix_seconds().unwrap(), (-1, 500_000_000));
}

#[test]
fn far_future_instants_are_representable() {
    // Year 9999: beyond many C/`time`-crate defaults.
    let t = Ticks::from_rfc3339("9999-12-31T23:59:59Z").unwrap();
    assert_eq!(t.to_civil_utc().unwrap().date().year(), 9999);
    assert_eq!(t.timestamp().unwrap(), 253_402_300_799);

    // Expanded years keep working well past 2038.
    let t = Ticks::from_rfc3339("10000-01-01T00:00:00Z").unwrap();
    assert_eq!(t.to_civil_utc().unwrap().date().year(), 10000);
    let t = Ticks::from_rfc3339("123456-01-01T00:00:00Z").unwrap();
    assert_eq!(t.to_civil_utc().unwrap().date().year(), 123456);

    // i64::MAX whole seconds: still a valid instant (timestamp accessors
    // work; only the civil projection is out of the `i32`-day range).
    let t = Ticks::from_timestamp(i64::MAX, 0).unwrap();
    assert_eq!(t.timestamp().unwrap(), i64::MAX);
    assert!(t.to_civil_utc().is_err()); // day count exceeds the civil range
}

#[test]
fn post_2038_text_round_trips() {
    for s in [
        "2038-01-19T03:14:07.999999999Z",
        "2038-01-19T03:14:08Z",
        "2039-01-19T03:14:08Z",
        "2100-02-28T12:00:00Z",
        "9999-12-31T23:59:59.123456789Z",
    ] {
        let t = Ticks::from_rfc3339(s).unwrap();
        let mut out = [0u8; 64];
        let n = t
            .write_rfc3339(&mut out, tzcraft::FractionDigits::Auto)
            .unwrap();
        let text = core::str::from_utf8(&out[..n]).unwrap();
        assert_eq!(Ticks::from_rfc3339(text).unwrap(), t, "{s}");
    }
}

#[test]
fn duration_seconds_are_i64() {
    // A `Duration` spanning the entire i64-second range is fine.
    let d = Duration::from_seconds(i64::MAX);
    assert_eq!(d.num_seconds().unwrap(), i64::MAX);
    // Instants can be shifted by more than 2^31 seconds in one step.
    let t = Ticks::EPOCH
        .checked_add(Duration::from_seconds(3_000_000_000))
        .unwrap();
    assert_eq!(t.timestamp().unwrap(), 3_000_000_000);
    assert_eq!(t.to_civil_utc().unwrap().date().year(), 2065);
}
