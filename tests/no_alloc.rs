//! Verifies the allocator-free surface.
//!
//! With the `alloc` feature **off**, every `String`-returning method
//! (`to_rfc3339`, `format`, ...) disappears from the API, but the core of
//! the library must keep working without an allocator: parsing, arithmetic,
//! `Display`/`FromStr`, and the `write_*` buffer APIs.
//!
//! This file is only compiled when `alloc` is off, so a green
//! `cargo test --no-default-features` is the proof.

#![cfg(not(feature = "alloc"))]

use core::str::FromStr;

use tzcraft::{CivilDateTime, Date, Duration, Offset, Ticks, TimeOfDay, Zone, Zoned};

#[test]
fn parsing_and_arithmetic_work_without_alloc() {
    let t = Ticks::from_rfc3339("2024-06-15T08:30:00.5Z").unwrap();
    // 2024-06-15T00:00:00Z = 1_718_409_600 s; +8 h 30 m = +30_600 s.
    assert_eq!(t.to_unix_seconds().unwrap(), (1_718_440_200, 500_000_000));
    assert_eq!(t.timestamp().unwrap(), 1_718_440_200);
    assert_eq!(t.to_civil_utc().unwrap().day(), 15);

    let d = Date::from_iso("2024-02-29").unwrap();
    assert_eq!((d.year(), d.month(), d.day()), (2024, 2, 29));
    assert_eq!(d.weekday(), tzcraft::Weekday::Thursday);

    let dur = Duration::from_iso8601("P1DT2H3M4.5S").unwrap();
    assert_eq!(dur.num_seconds().unwrap(), 93_784);
    assert_eq!(Offset::from_iso("+08:00").unwrap().as_seconds(), 28_800);
    assert_eq!(Zone::from_iso("UTC").unwrap(), Zone::Utc);
    assert_eq!(TimeOfDay::from_iso("23:59:59").unwrap().second(), 59);
    assert_eq!(
        CivilDateTime::from_iso("2024-06-15T08:30:00")
            .unwrap()
            .day(),
        15
    );
    let z = Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap();
    assert_eq!(z.offset().as_seconds(), 28_800);
    // 12:00 +08:00 is 04:00 UTC.
    assert_eq!(z.to_utc().to_civil_utc().unwrap().hour(), 4);
}

#[test]
fn from_str_works_without_alloc() {
    assert_eq!(
        Date::from_str("2024-02-29").unwrap(),
        Date::from_ymd(2024, 2, 29).unwrap()
    );
    assert_eq!(
        Ticks::from_str("2024-06-15T08:30:00Z").unwrap(),
        Ticks::from_rfc3339("2024-06-15T08:30:00Z").unwrap()
    );
    assert_eq!(Offset::from_str("Z").unwrap(), Offset::UTC);
    assert_eq!(
        Duration::from_str("PT1S").unwrap(),
        Duration::from_seconds(1)
    );
    assert_eq!(
        CivilDateTime::from_str("2024-01-01T00:00:00")
            .unwrap()
            .year(),
        2024
    );
}

#[test]
fn display_works_without_alloc() {
    // `Display` writes straight into the `fmt::Formatter`; no allocator.
    assert_eq!(
        format!("{}", Date::from_ymd(2024, 2, 29).unwrap()),
        "2024-02-29"
    );
    assert_eq!(
        format!(
            "{}",
            TimeOfDay::from_hms_nano(12, 0, 0, 500_000_000).unwrap()
        ),
        "12:00:00.5"
    );
    assert_eq!(format!("{}", Offset::from_hms(8, 0, 0).unwrap()), "+08:00");
    assert_eq!(format!("{}", Duration::from_seconds(90)), "PT1M30S");
    assert_eq!(
        format!(
            "{}",
            CivilDateTime::from_ymd_hms(2024, 1, 1, 0, 0, 0).unwrap()
        ),
        "2024-01-01T00:00:00"
    );
    assert_eq!(format!("{}", Zone::Utc), "UTC");
    assert_eq!(
        format!("{}", Zone::fixed(Offset::from_hms(-5, 30, 0).unwrap())),
        "-05:30"
    );
    assert_eq!(format!("{}", Ticks::EPOCH), "1970-01-01T00:00:00Z");
    assert_eq!(
        format!(
            "{}",
            Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap()
        ),
        "2024-06-15T12:00:00+08:00"
    );
    // Weekday / Month Display are pure table lookups.
    assert_eq!(format!("{}", tzcraft::Weekday::Thursday), "Thursday");
    assert_eq!(format!("{}", tzcraft::Month::February), "February");
}

#[test]
fn write_buf_apis_work_without_alloc() {
    let mut out = [0u8; 64];
    let n = Date::from_ymd(2024, 2, 29)
        .unwrap()
        .write_iso(&mut out)
        .unwrap();
    assert_eq!(core::str::from_utf8(&out[..n]).unwrap(), "2024-02-29");

    let n = Ticks::EPOCH
        .write_rfc3339(&mut out, tzcraft::FractionDigits::None)
        .unwrap();
    assert_eq!(
        core::str::from_utf8(&out[..n]).unwrap(),
        "1970-01-01T00:00:00Z"
    );

    let n = Duration::from_seconds(90).write_iso8601(&mut out).unwrap();
    assert_eq!(core::str::from_utf8(&out[..n]).unwrap(), "PT1M30S");

    let n = Ticks::EPOCH.write_format("%Y-%m-%d", &mut out).unwrap();
    assert_eq!(core::str::from_utf8(&out[..n]).unwrap(), "1970-01-01");

    let n = Offset::from_hms(-4, 30, 15)
        .unwrap()
        .write_iso(&mut out)
        .unwrap();
    assert_eq!(core::str::from_utf8(&out[..n]).unwrap(), "-04:30:15");

    // Buffer overflow is an error, never a panic or silent truncation.
    let mut tiny = [0u8; 2];
    assert!(Ticks::EPOCH
        .write_rfc3339(&mut tiny, tzcraft::FractionDigits::None)
        .is_err());
    assert!(Duration::from_seconds(90).write_iso8601(&mut tiny).is_err());
}
