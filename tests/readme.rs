//! Verifies that the code snippets shown in the README compile and run —
//! documentation that rots is documentation that lies.

#![cfg(all(feature = "serde", feature = "binary"))]

use tzcraft::{Date, Duration, Months, Offset, Ticks, Weekday, Zone, Zoned};

#[test]
fn readme_quick_start() -> Result<(), Box<dyn std::error::Error>> {
    // 一根时间轴，任意读数。
    let launch = Ticks::from_rfc3339("2024-06-15T08:30:00Z")?;
    let local = launch.to_zoned(Zone::fixed(Offset::from_hms(8, 0, 0)?));
    assert_eq!(
        local.to_rfc3339(tzcraft::FractionDigits::None),
        "2024-06-15T16:30:00+08:00"
    );
    assert_eq!(local.date()?.weekday(), Weekday::Saturday);

    // 日历感知的月份运算会钳制日期，而不是溢出。
    let jan = Date::from_ymd(2023, 1, 31)?;
    assert_eq!(jan.checked_add_months(Months::new(1))?, Date::from_ymd(2023, 2, 28)?);
    assert_eq!(jan.checked_add_months(Months::new(13))?, Date::from_ymd(2024, 2, 29)?); // 闰年

    // 时长带符号，ISO 8601 严格往返。
    let span = Duration::from_iso8601("P1DT2H3M4.5S")?;
    assert_eq!(span.to_iso8601(), "P1DT2H3M4.5S");

    // 文本和二进制共享同一套实现。
    let json = nextjson::nextencode(&local)?;
    let back: Zoned = nextjson::nextdecode(&json)?;
    assert_eq!(back, local);

    let bin = tzcraft::binary::encode(&local)?;
    let back: Zoned = tzcraft::binary::decode(&bin)?;
    assert_eq!(back, local);
    Ok(())
}

#[test]
fn readme_const_calendar() {
    const NEW_YEAR_2025: Date = Date::from_days_since_epoch(20_089);
    const WD: Weekday = NEW_YEAR_2025.weekday(); // 编译器算出来是星期三
    assert_eq!(WD, Weekday::Wednesday);
}

#[test]
fn readme_custom_zone_const() {
    const TOKYO: Zone = Zone::fixed(Offset::from_seconds_opt(9 * 3600).unwrap());
    assert_eq!(TOKYO.offset().as_seconds(), 9 * 3600);
}

#[derive(Debug, PartialEq, nextjson::NsonSerialize, nextjson::NsonDeserialize)]
struct Alarm {
    name: String,
    when: Zoned,
    repeat: tzcraft::Weekday,
    snooze: tzcraft::Duration,
}

#[test]
fn readme_derived_struct() {
    let alarm = Alarm {
        name: "wake up".into(),
        when: Zoned::from_rfc3339("2024-06-15T07:00:00+08:00").unwrap(),
        repeat: Weekday::Monday,
        snooze: Duration::from_minutes(9),
    };
    let json = nextjson::nextencode(&alarm).unwrap();
    let back: Alarm = nextjson::nextdecode(&json).unwrap();
    assert_eq!(back, alarm);
    let bin = tzcraft::binary::encode(&alarm).unwrap();
    let back: Alarm = tzcraft::binary::decode(&bin).unwrap();
    assert_eq!(back, alarm);
}
