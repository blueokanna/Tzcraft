//! Robustness harness: adversarial input must never panic.
//!
//! The parsers accept bytes from untrusted sources, so the contract here is
//! absolute: **no input of any length or content may cause a panic, an
//! unbounded allocation, or non-deterministic behaviour**. Every test asserts
//! that malformed input is rejected with an error (never a panic), and that
//! well-formed input round-trips.

use tzcraft::{CivilDateTime, Date, Days, Duration, Months, Offset, Ticks, TimeOfDay, Zone, Zoned};

/// A tiny deterministic PRNG (xorshift64*) so the harness needs no `rand`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn bytes(&mut self, len: usize, buf: &mut Vec<u8>) {
        buf.clear();
        for _ in 0..len {
            buf.push((self.next() & 0xFF) as u8);
        }
    }
}

/// Feed `s` through every entry point that parses bytes; the only acceptable
/// outcome is an `Err` (or, for deliberately well-formed input, an `Ok`).
fn poke(s: &str) {
    let _ = Date::from_iso(s);
    let _ = TimeOfDay::from_iso(s);
    let _ = CivilDateTime::from_iso(s);
    let _ = Ticks::from_rfc3339(s);
    let _ = Zoned::from_rfc3339(s);
    let _ = Offset::from_iso(s);
    let _ = Zone::from_iso(s);
    let _ = Duration::from_iso8601(s);
    let _ = Ticks::from_rfc2822(s);
    let _ = Zoned::from_rfc2822(s);
    let _ = Date::parse_from_str(s, "%Y-%m-%d");
    let _ = Date::parse_from_str(s, "%d/%m/%Y %H:%M:%S %z");
    let _ = Ticks::parse_from_str(s, "%Y-%m-%d %H:%M:%S %:z");
    let _ = Zoned::parse_from_str(s, "%Y-%m-%d %H:%M:%S %z");
    let _ = CivilDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f");
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut buf = Vec::new();
    for len in [0usize, 1, 2, 3, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        for _ in 0..200 {
            rng.bytes(len, &mut buf);
            // Only ASCII-random strings are guaranteed UTF-8; skip invalid
            // UTF-8 (the API contract is `&str`, so the caller owns that).
            let s = String::from_utf8_lossy(&buf);
            poke(&s);
        }
    }
}

#[test]
fn structured_adversarial_inputs() {
    let cases = [
        "",
        "%",
        "%Y",
        "%Q",
        "%Y-%m-%d %Q",
        "2024",
        "2024-",
        "2024-13-01",
        "2024-00-01",
        "2024-01-00",
        "2024-01-32",
        "99999999999999999999999999",
        "-99999999999999999999999999",
        "2024-01-01T99:99:99",
        "2024-01-01T00:00:00.",
        "2024-01-01T00:00:00.1234567890",
        "2024-01-01T00:00:00+99:99",
        "2024-01-01T00:00:00+24:00",
        "2024-01-01T00:00:00-24:00",
        "+08:60",
        "P",
        "PT",
        "PT.",
        "PT.S",
        "P1Y",
        "PT1H2D",
        "P999999999999999999999999999999999999999D",
        "Mon, 32 Foo 2024 00:00:00 +0000",
        "Tue, 1 Jan 2024 25:00:00 +0000",
        "Sun, 1 Jan 2023 00:00:00 +0000", // wrong weekday
        "\u{0}\u{1}\u{2}",
        "%%%%",
        "%s99999999999999999999",
        "2024-W53-1",
        "2024-W54-1",
    ];
    for s in cases {
        poke(s);
    }
}

#[test]
fn malformed_is_rejected_not_ignored() {
    // Spot checks that the parsers actually reject (not just avoid panicking).
    assert!(Date::from_iso("2024-13-01").is_err());
    assert!(Date::from_iso("2024-02-30").is_err());
    assert!(Ticks::from_rfc3339("2024-01-01T00:00:00").is_err()); // missing offset
    assert!(Ticks::from_rfc3339("2024-01-01T24:00:00Z").is_err());
    assert!(Duration::from_iso8601("P1Y").is_err());
    // Unknown timezone name in RFC 2822.
    assert!(Zoned::from_rfc2822("Mon, 1 Jan 2024 00:00:00 XYZ").is_err());
}

#[test]
fn long_inputs_are_bounded() {
    // Very long inputs must fail fast (or parse), never allocate unboundedly.
    let long = "9".repeat(100_000);
    let _ = Ticks::from_rfc3339(&long);
    let _ = Date::from_iso(&long);
    let _ = Duration::from_iso8601(&long);
    let _ = Date::parse_from_str(&long, &"%d".repeat(10_000));
    let _ = Ticks::parse_from_str(&long, &"%Y-%m-%d %H:%M:%S %z".repeat(5_000));
}

#[test]
fn malicious_format_strings_never_panic() {
    // Format strings are attacker-controllable in many systems; a hostile
    // template must never panic, hang, or allocate without bound.
    let d = Date::from_ymd(2024, 6, 15).unwrap();
    let dt = CivilDateTime::from_ymd_hms(2024, 6, 15, 12, 0, 0).unwrap();
    let t = Ticks::EPOCH;
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let mut buf = Vec::new();
    let corpus = [
        "%".to_string(),
        "%%".to_string(),
        "%99999999999999999999Y".to_string(),
        "%Y".repeat(5000),
        "%_%_%_%0%0%0".to_string(),
        "%-%-%#%#".to_string(),
        "%F%T%R%r%+%D%x%X".repeat(200),
        "%%Y%%m%%d".to_string(),
        "%.".to_string(),
        "%..f".to_string(),
        "%%.f".to_string(),
        "%999f".to_string(),
    ];
    for s in &corpus {
        let _ = d.format(s);
        let _ = dt.format(s);
        let _ = t.format(s);
    }
    for _ in 0..2000 {
        let len = 1 + (rng.next() % 32) as usize;
        rng.bytes(len, &mut buf);
        let s = String::from_utf8_lossy(&buf);
        let _ = d.format(&s);
        let _ = dt.format(&s);
        let _ = t.format(&s);
        // Random format strings used as parse templates must also be safe.
        let _ = Date::parse_from_str("2024-06-15", &s);
        let _ = Ticks::parse_from_str("2024-06-15T00:00:00Z", &s);
    }
}

#[test]
fn extreme_values_are_checked_not_wrapped() {
    // A u64 day count beyond i64 must error, not silently wrap negative.
    assert!(Ticks::EPOCH.checked_add_days(Days::new(u64::MAX)).is_err());
    assert!(Ticks::EPOCH.checked_sub_days(Days::new(u64::MAX)).is_err());
    let z = Zoned::new(Ticks::EPOCH, Zone::Utc);
    assert!(z.checked_add_days(Days::new(u64::MAX)).is_err());
    assert!(z.checked_sub_days(Days::new(u64::MAX)).is_err());

    // Huge unit-scaled durations must not panic or wrap.
    let _ = Duration::from_days(i64::MAX);
    let _ = Duration::from_minutes(i64::MAX);
    let _ = Duration::from_hours(i64::MAX);
    let _ = Duration::weeks(i64::MAX);
    let _ = Duration::from_seconds(i64::MIN);

    // Huge month counts must error (range), never panic.
    let d = Date::from_ymd(1, 1, 1).unwrap();
    assert!(d.checked_add_months(Months::new(u32::MAX)).is_err());
    assert!(d.checked_sub_months(Months::new(u32::MAX)).is_err());

    // Duration arithmetic saturates/errors at the boundary.
    assert_eq!(
        Duration::from_nanos(i128::MAX).checked_add(Duration::from_nanos(1)),
        Err(tzcraft::Error::overflow())
    );

    // i128 addition paths must not overflow: MAX - MIN would wrap in release
    // and panic in debug; it must saturate instead.
    let _ = Ticks::MAX.duration_since(Ticks::MIN);
    let _ = Ticks::MIN.duration_since(Ticks::MAX);

    // Checked arithmetic must report overflow for absurdly large durations.
    let huge = Duration::from_nanos(i128::MAX);
    let dt = CivilDateTime::from_ymd_hms(2024, 1, 1, 0, 0, 0).unwrap();
    assert!(dt.checked_add(huge).is_err());
    assert!(dt.checked_sub(huge).is_err());
    let tod = TimeOfDay::MIDNIGHT;
    assert!(tod.checked_add(huge).is_err());
    assert!(tod.checked_sub(huge).is_err());
    // Overflowing add saturates instead of wrapping.
    let _ = tod.overflowing_add(huge);
    let _ = tod.overflowing_add(Duration::from_nanos(i128::MIN));
}

#[test]
fn rfc2822_round_trips() {
    let t = Ticks::from_rfc3339("2024-01-01T10:52:37Z").unwrap();
    assert_eq!(t.to_rfc2822(), "Mon,  1 Jan 2024 10:52:37 +0000");
    assert_eq!(Ticks::from_rfc2822(&t.to_rfc2822()).unwrap(), t);

    let z = Zoned::from_rfc3339("2024-06-15T07:00:00+08:00").unwrap();
    assert_eq!(z.to_rfc2822(), "Sat, 15 Jun 2024 07:00:00 +0800");
    assert_eq!(Zoned::from_rfc2822(&z.to_rfc2822()).unwrap(), z);

    // Named zones, obsolete 2-digit years, and missing seconds all parse.
    let z = Zoned::from_rfc2822("Mon, 1 Jan 24 10:52 GMT").unwrap();
    assert_eq!(
        z.to_utc().to_rfc3339(tzcraft::FractionDigits::None),
        "2024-01-01T10:52:00Z"
    );
    let z = Zoned::from_rfc2822("1 Jan 2024 10:52:37 -0000").unwrap();
    assert_eq!(z.zone(), Zone::Utc);
    let z = Zoned::from_rfc2822("Tue, 1 Jul 2003 10:52:37 +0200").unwrap();
    assert_eq!(z.offset().as_seconds(), 2 * 3600);
}

#[test]
fn strftime_edge_cases() {
    let d = Date::from_ymd(2024, 12, 31).unwrap();
    // Week-of-year variants.
    assert_eq!(d.format("%U").unwrap(), "52"); // Sunday-based week
    assert_eq!(d.format("%W").unwrap(), "53"); // Monday-based week (2024-12-31 is Tuesday)
                                               // 12-hour clock + AM/PM round trip.
    let dt = CivilDateTime::from_ymd_hms(2024, 6, 15, 23, 5, 7).unwrap();
    assert_eq!(dt.format("%I:%M %p").unwrap(), "11:05 PM");
    assert_eq!(dt.format("%-I:%M %P").unwrap(), "11:05 pm");
    let parsed = TimeOfDay::parse_from_str("11:05 PM", "%I:%M %p").unwrap();
    assert_eq!((parsed.hour(), parsed.minute()), (23, 5));
    // Fraction variants.
    let dt = CivilDateTime::from_ymd_hms_nano(2024, 6, 15, 12, 0, 0, 500_000_000).unwrap();
    assert_eq!(dt.format("%f").unwrap(), "500000000");
    assert_eq!(dt.format("%.f").unwrap(), ".5");
    assert_eq!(dt.format("%.3f").unwrap(), ".500");
    assert_eq!(dt.format("%.9f").unwrap(), ".500000000");
    // %s on ticks.
    let t = Ticks::from_rfc3339("2024-06-15T00:00:00Z").unwrap();
    assert_eq!(t.format("%s").unwrap(), "1718409600");
    assert_eq!(Ticks::parse_from_str("1718409600", "%s").unwrap(), t);
}
