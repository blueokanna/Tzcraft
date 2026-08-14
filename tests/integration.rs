//! Integration tests: nextjson text codec, rustbinary binary codec, and
//! derived composite structures that mix `tzcraft` types.

#![cfg(all(feature = "serde", feature = "binary"))]

use tzcraft::{
    CivilDateTime, Date, Duration, FractionDigits, Month, Offset, Ticks, TimeOfDay, Weekday, Zone,
    Zoned,
};

fn json_round_trip<T>(value: &T) -> T
where
    T: nextjson::NsonSerialize
        + for<'de> nextjson::NsonDeserialize<'de>
        + PartialEq
        + core::fmt::Debug,
{
    let bytes = nextjson::nextencode(value).expect("json encode");
    let back: T = nextjson::nextdecode(&bytes).expect("json decode");
    assert_eq!(&back, value, "json round trip");
    back
}

fn bin_round_trip<T>(value: &T) -> T
where
    T: nextjson::NsonSerialize
        + for<'de> nextjson::NsonDeserialize<'de>
        + PartialEq
        + core::fmt::Debug,
{
    let bytes = tzcraft::binary::encode(value).expect("binary encode");
    let back: T = tzcraft::binary::decode(&bytes).expect("binary decode");
    assert_eq!(&back, value, "binary round trip");
    back
}

#[test]
fn json_round_trips_every_type() {
    json_round_trip(&Ticks::from_rfc3339("2024-06-15T08:30:00Z").unwrap());
    json_round_trip(&Ticks::from_rfc3339("1969-12-31T23:59:59.123456789Z").unwrap());
    json_round_trip(&Duration::from_iso8601("P1DT2H3M4.5S").unwrap());
    json_round_trip(&Duration::from_iso8601("-PT2H").unwrap());
    json_round_trip(&Date::from_ymd(2024, 6, 15).unwrap());
    json_round_trip(&Date::from_ymd(-1, 12, 31).unwrap());
    json_round_trip(&TimeOfDay::from_hms_nano(23, 59, 59, 123_456_789).unwrap());
    json_round_trip(&CivilDateTime::from_iso("2024-06-15T08:30:00.5").unwrap());
    json_round_trip(&Offset::from_hms(-5, 30, 0).unwrap());
    json_round_trip(&Zone::fixed(Offset::from_hms(8, 0, 0).unwrap()));
    json_round_trip(&Zone::Utc);
    json_round_trip(&Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap());
    json_round_trip(&Zoned::from_rfc3339("2024-06-15T12:00:00Z").unwrap());
    json_round_trip(&Weekday::Friday);
    json_round_trip(&Month::October);
}

#[test]
fn binary_round_trips_every_type() {
    bin_round_trip(&Ticks::from_rfc3339("2024-06-15T08:30:00Z").unwrap());
    bin_round_trip(&Duration::from_iso8601("P1DT2H3M4.5S").unwrap());
    bin_round_trip(&Date::from_ymd(2024, 6, 15).unwrap());
    bin_round_trip(&TimeOfDay::from_hms_nano(23, 59, 59, 123_456_789).unwrap());
    bin_round_trip(&CivilDateTime::from_iso("2024-06-15T08:30:00.5").unwrap());
    bin_round_trip(&Offset::from_hms(-5, 30, 0).unwrap());
    bin_round_trip(&Zone::fixed(Offset::from_hms(8, 0, 0).unwrap()));
    bin_round_trip(&Zone::Utc);
    bin_round_trip(&Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap());
    bin_round_trip(&Weekday::Sunday);
    bin_round_trip(&Month::February);
}

#[test]
fn json_human_shapes_are_readable() {
    assert_eq!(
        nextjson::to_string(&Date::from_ymd(2024, 6, 15).unwrap()).unwrap(),
        "\"2024-06-15\""
    );
    assert_eq!(
        nextjson::to_string(&Ticks::from_rfc3339("2024-06-15T08:30:00Z").unwrap()).unwrap(),
        "\"2024-06-15T08:30:00Z\""
    );
    assert_eq!(
        nextjson::to_string(&Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap()).unwrap(),
        "\"2024-06-15T12:00:00+08:00\""
    );
    assert_eq!(
        nextjson::to_string(&Duration::from_seconds(90)).unwrap(),
        "\"PT1M30S\""
    );
    assert_eq!(nextjson::to_string(&Weekday::Friday).unwrap(), "\"Friday\"");
    assert_eq!(nextjson::to_string(&Month::October).unwrap(), "\"October\"");
    assert_eq!(nextjson::to_string(&Offset::UTC).unwrap(), "\"Z\"");
}

#[test]
fn binary_is_compact() {
    // The compact profile stores scalars with no type tag; a date is just the
    // 4-byte day count and a tick is the 16-byte nanosecond count.
    let date = Date::from_ymd(2024, 6, 15).unwrap();
    let ticks = Ticks::from_rfc3339("2024-06-15T08:30:00Z").unwrap();
    assert!(tzcraft::binary::encoded_size(&date).unwrap() <= 8);
    assert!(tzcraft::binary::encoded_size(&ticks).unwrap() <= 24);
    assert!(
        tzcraft::binary::encoded_size(&ticks).unwrap()
            > tzcraft::binary::encoded_size(&date).unwrap()
    );
}

#[test]
fn lenient_weekday_month_decode() {
    // Human decode accepts a number where a name is expected.
    let json = b"3";
    let wd: Weekday = nextjson::nextdecode(json).unwrap();
    assert_eq!(wd, Weekday::Thursday);
    let json = b"9";
    let mo: Month = nextjson::nextdecode(json).unwrap();
    assert_eq!(mo, Month::September);
}

#[derive(Debug, PartialEq, nextjson::NsonSerialize, nextjson::NsonDeserialize)]
struct Alarm {
    name: String,
    when: Zoned,
    repeat: Weekday,
    snooze: Duration,
    created: Ticks,
}

#[test]
fn derived_struct_round_trips_both_codecs() {
    let alarm = Alarm {
        name: "wake up".into(),
        when: Zoned::from_rfc3339("2024-06-15T07:00:00+08:00").unwrap(),
        repeat: Weekday::Monday,
        snooze: Duration::from_minutes(9),
        created: Ticks::from_rfc3339("2024-06-01T00:00:00Z").unwrap(),
    };

    let json = nextjson::nextencode(&alarm).unwrap();
    assert_eq!(
        String::from_utf8(json.clone()).unwrap(),
        r#"{"name":"wake up","when":"2024-06-15T07:00:00+08:00","repeat":"Monday","snooze":"PT9M","created":"2024-06-01T00:00:00Z"}"#
    );
    let back: Alarm = nextjson::nextdecode(&json).unwrap();
    assert_eq!(back, alarm);

    let bin = tzcraft::binary::encode(&alarm).unwrap();
    let back: Alarm = tzcraft::binary::decode(&bin).unwrap();
    assert_eq!(back, alarm);
}

#[test]
fn json_value_round_trip() {
    // `tzcraft` types work through nextjson's dynamic `Value` as well.
    let z = Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap();
    let value = nextjson::to_value(&z).unwrap();
    assert_eq!(value, nextjson::Value::from("2024-06-15T12:00:00+08:00"));
    let back: Zoned = nextjson::from_value(value).unwrap();
    assert_eq!(back, z);
}

#[test]
fn rfc3339_zoned_utc_normalization() {
    // A zero offset round-trips as UTC zone but keeps the explicit form.
    let z = Zoned::from_rfc3339("2024-06-15T12:00:00+00:00").unwrap();
    assert_eq!(z.zone(), Zone::Utc);
    assert_eq!(z.to_rfc3339(FractionDigits::None), "2024-06-15T12:00:00Z");
}
