//! `chrono`-replacement verification.
//!
//! These tests express the everyday `chrono` usage patterns — the API that
//! real applications depend on — in `tzcraft`, and prove the migration
//! surface works end to end. Where a `chrono` name differs, the mapping is
//! documented in the README.

#![cfg(feature = "std")]

use tzcraft::{
    CivilDateTime, Date, Days, Duration, Months, Offset, Ticks, TimeOfDay, Weekday, Zone, Zoned,
};

#[test]
fn duration_chrono_constructors_and_counts() {
    assert_eq!(
        Duration::minutes(1)
            .checked_add(Duration::seconds(30))
            .unwrap(),
        Duration::seconds(90)
    );
    assert_eq!(Duration::hours(2).num_minutes().unwrap(), 120);
    assert_eq!(Duration::days(1).num_hours().unwrap(), 24);
    assert_eq!(Duration::weeks(1).num_days().unwrap(), 7);
    assert_eq!(Duration::milliseconds(1500).num_seconds().unwrap(), 1);
    assert_eq!(Duration::nanoseconds(5).num_nanoseconds().unwrap(), 5);
    assert!(Duration::seconds(-90).is_negative());
    assert!(Duration::ZERO.is_zero());
    let std_dur = Duration::seconds(3).to_std().unwrap();
    assert_eq!(Duration::from_std(std_dur), Duration::seconds(3));
    assert!(Duration::seconds(-1).to_std().is_err());
}

#[test]
fn date_chrono_patterns() {
    // chrono::NaiveDate::from_ymd_opt(...).unwrap()
    let d = Date::from_ymd(2024, 2, 29).unwrap();

    // chrono: d.format("%Y-%m-%d").to_string()
    assert_eq!(d.format("%Y-%m-%d").unwrap(), "2024-02-29");
    assert_eq!(d.weekday(), Weekday::Thursday);
    assert_eq!(d.ordinal(), 60);
    assert_eq!(d.iso_week().week(), 9);
    assert_eq!(d.iso_week().year(), 2024);

    // chrono: d.checked_add_months(Months::new(1))
    assert_eq!(
        d.checked_add_months(Months::new(1)).unwrap(),
        Date::from_ymd(2024, 3, 29).unwrap()
    );
    assert_eq!(
        d.checked_sub_months(Months::new(1)).unwrap(),
        Date::from_ymd(2024, 1, 29).unwrap()
    );

    // chrono: d.checked_add_days(Days::new(1))
    assert_eq!(
        d.checked_add_days(Days::new(1)).unwrap(),
        Date::from_ymd(2024, 3, 1).unwrap()
    );
    assert_eq!(
        d.checked_sub_days(Days::new(1)).unwrap(),
        Date::from_ymd(2024, 2, 28).unwrap()
    );

    // chrono: NaiveDate::parse_from_str
    assert_eq!(Date::parse_from_str("29.02.2024", "%d.%m.%Y").unwrap(), d);
    assert_eq!(
        Date::parse_from_str("Thu Feb 29 2024", "%a %b %d %Y").unwrap(),
        d
    );

    // chrono: with_year / with_month / with_day
    assert_eq!(d.with_year(2028).unwrap().year(), 2028);
    assert_eq!(d.with_month(3).unwrap().month(), 3);
    assert_eq!(d.with_day(1).unwrap().day(), 1);
}

#[test]
fn datetime_chrono_patterns() {
    // chrono: date.and_hms_opt(12, 0, 0)
    let dt = Date::from_ymd(2024, 2, 29)
        .unwrap()
        .and_hms(12, 0, 0)
        .unwrap();

    // chrono: dt.format("%Y-%m-%d %H:%M:%S")
    assert_eq!(
        dt.format("%Y-%m-%d %H:%M:%S").unwrap(),
        "2024-02-29 12:00:00"
    );

    // chrono: dt.checked_add_signed(chrono::Duration::days(1))
    assert_eq!(
        dt.checked_add_signed(Duration::days(1)).unwrap().date(),
        Date::from_ymd(2024, 3, 1).unwrap()
    );

    // chrono: dt.signed_duration_since(...)
    assert_eq!(
        dt.signed_duration_since(dt.checked_sub_signed(Duration::hours(2)).unwrap()),
        Duration::hours(2)
    );

    // chrono: NaiveDateTime::parse_from_str + format round trip
    let parsed = CivilDateTime::parse_from_str("2024-02-29 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    assert_eq!(parsed, dt);

    // chrono: NaiveDateTime::from_timestamp_opt
    assert_eq!(
        CivilDateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        CivilDateTime::from_ymd_hms(2023, 11, 14, 22, 13, 20).unwrap()
    );
}

#[test]
fn time_chrono_patterns() {
    let t = TimeOfDay::from_hms_nano(23, 59, 59, 123_456_789).unwrap();
    assert_eq!(t.format("%H:%M:%S%.f").unwrap(), "23:59:59.123456789");
    assert_eq!(t.format("%I:%M %p").unwrap(), "11:59 PM");
    assert_eq!(t.num_seconds_from_midnight(), 86_399);
    assert_eq!(t.hour(), 23);
    assert_eq!(t.minute(), 59);
    assert_eq!(t.second(), 59);
    assert_eq!(t.nanosecond(), 123_456_789);
    assert_eq!(t.with_hour(0).unwrap().hour(), 0);
    let parsed = TimeOfDay::parse_from_str("11:59 PM", "%I:%M %p").unwrap();
    assert_eq!(parsed, TimeOfDay::from_hms(23, 59, 0).unwrap());
}

#[test]
fn instant_chrono_patterns() {
    // chrono::Utc::now().timestamp()
    let now = Ticks::now().unwrap();
    assert!(now.timestamp().unwrap() > 1_600_000_000);
    assert!(now.timestamp_millis().unwrap() > 1_600_000_000_000);

    // chrono: DateTime::<Utc>::from_timestamp(secs, nsecs)
    let t = Ticks::from_timestamp(1_700_000_000, 0).unwrap();
    assert_eq!(
        t.to_rfc3339(tzcraft::FractionDigits::None),
        "2023-11-14T22:13:20Z"
    );

    // chrono: DateTime::parse_from_rfc3339 / parse_from_rfc2822
    let z = Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap();
    assert_eq!(
        z.format("%Y-%m-%d %H:%M:%S %:z").unwrap(),
        "2024-06-15 12:00:00 +08:00"
    );
    assert_eq!(
        z.timestamp().unwrap(),
        Ticks::from_rfc3339("2024-06-15T04:00:00Z")
            .unwrap()
            .timestamp()
            .unwrap()
    );

    // chrono: dt.with_timezone(Utc)
    let utc = z.with_zone(Zone::Utc);
    assert_eq!(
        utc.to_rfc3339(tzcraft::FractionDigits::None),
        "2024-06-15T04:00:00Z"
    );

    // chrono: DateTime::naive_utc() / naive_local() equivalents
    assert_eq!(z.civil().unwrap().to_iso(), "2024-06-15T12:00:00");
    assert_eq!(
        z.to_utc().to_civil_utc().unwrap().to_iso(),
        "2024-06-15T04:00:00"
    );

    // chrono: DateTime::parse_from_str with a fixed offset
    let parsed =
        Zoned::parse_from_str("2024-06-15 12:00:00 +0800", "%Y-%m-%d %H:%M:%S %z").unwrap();
    assert_eq!(parsed, z);
}

#[test]
fn offset_chrono_patterns() {
    // chrono: FixedOffset::east_opt / from_hms_opt
    let off = Offset::from_hms(-5, 30, 0).unwrap();
    assert_eq!(off.as_seconds(), -(5 * 3600 + 30 * 60));
    assert_eq!(off.as_hms(), (-5, 30, 0));
    assert_eq!(off.to_iso(), "-05:30");
    assert!(Offset::from_seconds(86_400).is_err());
    assert!(Offset::from_seconds(-86_400).is_err());
}
