//! Const-evaluable proleptic Gregorian civil-calendar math.
//!
//! Every function here is `const fn`, so the compiler folds calendar
//! arithmetic at compile time: a `const` date, a static zone table, or an
//! array of ISO week numbers can all rely on the same engine that runtime
//! code uses. There is no delegation to `chrono`, `time`, or IANA data — the
//! leap rules, the day-count inversion and the ISO week projection are all
//! spelled out below from first principles.
//!
//! The proleptic Gregorian calendar is the only calendar: no Julian, no
//! Hebrew, no ISO-year-as-calendar split. Weekdays use ISO 8601 ordering
//! (Monday first). The epoch used by day counts is `1970-01-01`, matching
//! the Unix timeline that [`crate::Ticks`] is built on.

use core::fmt;
use core::str::FromStr;

use crate::error::{Error, Result};

/// Nanoseconds per second.
pub(crate) const NS_PER_SEC: i128 = 1_000_000_000;
/// Nanoseconds per minute.
pub(crate) const NS_PER_MIN: i128 = 60 * NS_PER_SEC;
/// Nanoseconds per hour.
pub(crate) const NS_PER_HOUR: i128 = 60 * NS_PER_MIN;
/// Nanoseconds per civil day.
pub(crate) const NS_PER_DAY: i128 = 24 * NS_PER_HOUR;
/// Seconds per civil day.
pub(crate) const SECS_PER_DAY: i64 = 86_400;
/// Days from the proleptic year 0000 to 1970-01-01.
pub(crate) const DAYS_0000_TO_1970: i64 = 719_468;
/// Days from `0001-01-01` (the Common Era epoch) to 1970-01-01.
pub(crate) const DAYS_0001_TO_1970: i64 = 719_162;

/// Floor division (rounds toward negative infinity).
///
/// `const fn`-safe on every Rust version that ships the integer `%` and `/`
/// operators, which is why it exists instead of relying on `div_euclid`
/// const-stabilization.
#[inline]
pub(crate) const fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    let r = a % b;
    if r != 0 && (r < 0) != (b < 0) {
        q - 1
    } else {
        q
    }
}

/// Floor modulo (result has the sign of the divisor).
#[inline]
pub(crate) const fn floor_mod(a: i64, b: i64) -> i64 {
    let r = a % b;
    if r != 0 && (r < 0) != (b < 0) {
        r + b
    } else {
        r
    }
}

/// Floor division of a timeline quantity (`i128` nanoseconds) by `unit`.
///
/// For non-negative operands plain `/` is used — a single instruction pair —
/// instead of `div_euclid`'s branch-and-adjust sequence. Both branches have
/// identical results.
#[inline]
pub(crate) fn floor_div_ns(ns: i128, unit: i128) -> i128 {
    if ns >= 0 {
        ns / unit
    } else {
        ns.div_euclid(unit)
    }
}

/// Floor remainder of a timeline quantity by `unit`, in `0..unit`.
#[inline]
pub(crate) fn floor_rem_ns(ns: i128, unit: i128) -> i128 {
    if ns >= 0 {
        ns % unit
    } else {
        ns.rem_euclid(unit)
    }
}

/// Decompose nanoseconds since the Unix epoch into `(floor_days, ns_of_day)`
/// with `ns_of_day` in `0..NS_PER_DAY`. The single projection both [`Ticks`]
/// and [`CivilDateTime`] route their timeline arithmetic through, so the
/// floor/carry semantics live in exactly one place.
///
/// [`Ticks`]: crate::Ticks
/// [`CivilDateTime`]: crate::CivilDateTime
#[inline]
pub(crate) fn ns_divmod_day(ns: i128) -> (i128, i128) {
    (floor_div_ns(ns, NS_PER_DAY), floor_rem_ns(ns, NS_PER_DAY))
}

/// Whether `year` is a leap year in the proleptic Gregorian calendar.
#[inline]
pub(crate) const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Number of days in `month` (1-based) of `year`. Returns `0` for an invalid
/// month; callers validate before use.
#[inline]
pub(crate) const fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Number of days in `year`.
#[inline]
pub(crate) const fn days_in_year(year: i32) -> u32 {
    if is_leap_year(year) {
        366
    } else {
        365
    }
}

/// Days since `1970-01-01` for a valid civil date `(year, month, day)`.
///
/// `month` must be in `1..=12` and `day` must be valid for the month; the
/// public constructors validate before calling. The proleptic Gregorian
/// calendar includes year 0 (1 BCE), so negative years are meaningful.
#[inline]
pub(crate) const fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;
    // Shift January/February into the previous year so March is month 0;
    // this makes the leap-day adjustment uniform across the year.
    let y = if m <= 2 { y - 1 } else { y };
    let era = floor_div(y, 400);
    let yoe = y - era * 400; // year-of-era, [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // month index, [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // day-of-year, 0-based, March = 0
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // day-of-era, 0-based
    era * 146_097 + doe - DAYS_0000_TO_1970
}

/// Inverse of [`days_from_civil`]: `(year, month, day)` for a day count.
///
/// The 400-year cycle has exactly 146,097 days; each era is decomposed into a
/// year and a day-of-year, then the day-of-year is decomposed into a month
/// and a day using the `153*m` table shared with [`days_from_civil`].
#[inline]
pub(crate) const fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + DAYS_0000_TO_1970;
    let era = floor_div(z, 146_097);
    let doe = z - era * 146_097; // [0, 146096]
                                 // Year-of-era from day-of-era; the correction terms account for the
                                 // 100-year and 400-year leap rules without branching.
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Weekday of a day count (`Monday = 0`). Derived from the anchor that
/// `1970-01-01` was a Thursday.
#[inline]
pub(crate) const fn weekday_from_civil(days: i64) -> Weekday {
    match floor_mod(days + 3, 7) {
        0 => Weekday::Monday,
        1 => Weekday::Tuesday,
        2 => Weekday::Wednesday,
        3 => Weekday::Thursday,
        4 => Weekday::Friday,
        5 => Weekday::Saturday,
        _ => Weekday::Sunday,
    }
}

/// 1-based day-of-year for a valid civil date.
#[inline]
pub(crate) const fn day_of_year(year: i32, month: u32, day: u32) -> u32 {
    (days_from_civil(year, month, day) - days_from_civil(year, 1, 1) + 1) as u32
}

/// ISO 8601 week date `(iso_year, week)` for a valid civil date.
///
/// Week 1 is the week containing the first Thursday; the ISO year therefore
/// differs from the calendar year for the first days of January and the last
/// days of December. The projection goes through the Thursday of the current
/// Mon–Sun week (which fixes the ISO year), then counts weeks from the Monday
/// of the week containing January 4 of that ISO year.
#[inline]
pub(crate) const fn iso_week_from_civil(year: i32, month: u32, day: u32) -> (i32, u32) {
    let z = days_from_civil(year, month, day);
    let wd = weekday_from_civil(z) as i64;
    // Thursday of the current week fixes the ISO year.
    let thursday = z + (3 - wd);
    let (iso_year, _, _) = civil_from_days(thursday);
    // Monday of the week containing Jan 4 of the ISO year.
    let jan4 = days_from_civil(iso_year, 1, 4);
    let jan4_wd = weekday_from_civil(jan4) as i64;
    let monday_week1 = jan4 - jan4_wd;
    let week = floor_div(z - monday_week1, 7) + 1;
    (iso_year, week as u32)
}

/// A weekday, ISO 8601 order (Monday first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Weekday {
    /// Monday
    Monday = 0,
    /// Tuesday
    Tuesday = 1,
    /// Wednesday
    Wednesday = 2,
    /// Thursday
    Thursday = 3,
    /// Friday
    Friday = 4,
    /// Saturday
    Saturday = 5,
    /// Sunday
    Sunday = 6,
}

impl Weekday {
    /// ISO 8601 number: Monday = 1 through Sunday = 7.
    pub const fn number_from_monday(self) -> u8 {
        (self as u8) + 1
    }

    /// Sunday-based number: Sunday = 1 through Saturday = 7.
    pub const fn number_from_sunday(self) -> u8 {
        ((self as u8) + 1) % 7 + 1
    }

    /// Weekday for an ISO 8601 number (1 = Monday .. 7 = Sunday).
    pub const fn from_iso_number(n: u32) -> Option<Weekday> {
        match n {
            1 => Some(Weekday::Monday),
            2 => Some(Weekday::Tuesday),
            3 => Some(Weekday::Wednesday),
            4 => Some(Weekday::Thursday),
            5 => Some(Weekday::Friday),
            6 => Some(Weekday::Saturday),
            7 => Some(Weekday::Sunday),
            _ => None,
        }
    }

    /// Weekday for the internal discriminant (0 = Monday .. 6 = Sunday).
    pub const fn from_discriminant(n: u8) -> Option<Weekday> {
        match n {
            0 => Some(Weekday::Monday),
            1 => Some(Weekday::Tuesday),
            2 => Some(Weekday::Wednesday),
            3 => Some(Weekday::Thursday),
            4 => Some(Weekday::Friday),
            5 => Some(Weekday::Saturday),
            6 => Some(Weekday::Sunday),
            _ => None,
        }
    }

    /// Weekday for a case-sensitive English name such as `"Monday"`.
    pub fn from_name(name: &str) -> Option<Weekday> {
        match name {
            "Monday" => Some(Weekday::Monday),
            "Tuesday" => Some(Weekday::Tuesday),
            "Wednesday" => Some(Weekday::Wednesday),
            "Thursday" => Some(Weekday::Thursday),
            "Friday" => Some(Weekday::Friday),
            "Saturday" => Some(Weekday::Saturday),
            "Sunday" => Some(Weekday::Sunday),
            _ => None,
        }
    }

    /// Next weekday, wrapping Sunday → Monday.
    pub const fn succ(self) -> Weekday {
        match self {
            Weekday::Monday => Weekday::Tuesday,
            Weekday::Tuesday => Weekday::Wednesday,
            Weekday::Wednesday => Weekday::Thursday,
            Weekday::Thursday => Weekday::Friday,
            Weekday::Friday => Weekday::Saturday,
            Weekday::Saturday => Weekday::Sunday,
            Weekday::Sunday => Weekday::Monday,
        }
    }

    /// Previous weekday, wrapping Monday → Sunday.
    pub const fn pred(self) -> Weekday {
        match self {
            Weekday::Monday => Weekday::Sunday,
            Weekday::Tuesday => Weekday::Monday,
            Weekday::Wednesday => Weekday::Tuesday,
            Weekday::Thursday => Weekday::Wednesday,
            Weekday::Friday => Weekday::Thursday,
            Weekday::Saturday => Weekday::Friday,
            Weekday::Sunday => Weekday::Saturday,
        }
    }

    /// English name.
    pub const fn name(self) -> &'static str {
        match self {
            Weekday::Monday => "Monday",
            Weekday::Tuesday => "Tuesday",
            Weekday::Wednesday => "Wednesday",
            Weekday::Thursday => "Thursday",
            Weekday::Friday => "Friday",
            Weekday::Saturday => "Saturday",
            Weekday::Sunday => "Sunday",
        }
    }

    /// Abbreviated English name (`Mon`..`Sun`), for `%a` formatting.
    pub const fn short_name(self) -> &'static str {
        match self {
            Weekday::Monday => "Mon",
            Weekday::Tuesday => "Tue",
            Weekday::Wednesday => "Wed",
            Weekday::Thursday => "Thu",
            Weekday::Friday => "Fri",
            Weekday::Saturday => "Sat",
            Weekday::Sunday => "Sun",
        }
    }

    /// Weekday for a 3-letter abbreviation (`Mon`..`Sun`), case-sensitive.
    pub fn from_short_name(name: &str) -> Option<Weekday> {
        match name {
            "Mon" => Some(Weekday::Monday),
            "Tue" => Some(Weekday::Tuesday),
            "Wed" => Some(Weekday::Wednesday),
            "Thu" => Some(Weekday::Thursday),
            "Fri" => Some(Weekday::Friday),
            "Sat" => Some(Weekday::Saturday),
            "Sun" => Some(Weekday::Sunday),
            _ => None,
        }
    }

    /// Internal discriminant (0 = Monday .. 6 = Sunday).
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for Weekday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Weekday {
    type Err = Error;

    fn from_str(s: &str) -> Result<Weekday> {
        Weekday::from_name(s).ok_or_else(|| Error::invalid("unknown weekday name"))
    }
}

/// A month of the proleptic Gregorian year.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Month {
    /// January
    January = 1,
    /// February
    February = 2,
    /// March
    March = 3,
    /// April
    April = 4,
    /// May
    May = 5,
    /// June
    June = 6,
    /// July
    July = 7,
    /// August
    August = 8,
    /// September
    September = 9,
    /// October
    October = 10,
    /// November
    November = 11,
    /// December
    December = 12,
}

impl Month {
    /// Month for a 1-based number (1 = January .. 12 = December).
    pub const fn from_u32(n: u32) -> Option<Month> {
        match n {
            1 => Some(Month::January),
            2 => Some(Month::February),
            3 => Some(Month::March),
            4 => Some(Month::April),
            5 => Some(Month::May),
            6 => Some(Month::June),
            7 => Some(Month::July),
            8 => Some(Month::August),
            9 => Some(Month::September),
            10 => Some(Month::October),
            11 => Some(Month::November),
            12 => Some(Month::December),
            _ => None,
        }
    }

    /// Month for a case-sensitive English name such as `"January"`.
    pub fn from_name(name: &str) -> Option<Month> {
        match name {
            "January" => Some(Month::January),
            "February" => Some(Month::February),
            "March" => Some(Month::March),
            "April" => Some(Month::April),
            "May" => Some(Month::May),
            "June" => Some(Month::June),
            "July" => Some(Month::July),
            "August" => Some(Month::August),
            "September" => Some(Month::September),
            "October" => Some(Month::October),
            "November" => Some(Month::November),
            "December" => Some(Month::December),
            _ => None,
        }
    }

    /// 1-based month number.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Number of days in this month for `year`.
    pub const fn num_days(self, year: i32) -> u32 {
        days_in_month(year, self as u32)
    }

    /// Previous month, wrapping December → January.
    pub const fn prev(self) -> Month {
        match self {
            Month::January => Month::December,
            Month::February => Month::January,
            Month::March => Month::February,
            Month::April => Month::March,
            Month::May => Month::April,
            Month::June => Month::May,
            Month::July => Month::June,
            Month::August => Month::July,
            Month::September => Month::August,
            Month::October => Month::September,
            Month::November => Month::October,
            Month::December => Month::November,
        }
    }

    /// Next month, wrapping January → February, December → January.
    pub const fn next(self) -> Month {
        match self {
            Month::January => Month::February,
            Month::February => Month::March,
            Month::March => Month::April,
            Month::April => Month::May,
            Month::May => Month::June,
            Month::June => Month::July,
            Month::July => Month::August,
            Month::August => Month::September,
            Month::September => Month::October,
            Month::October => Month::November,
            Month::November => Month::December,
            Month::December => Month::January,
        }
    }

    /// English name.
    pub const fn name(self) -> &'static str {
        match self {
            Month::January => "January",
            Month::February => "February",
            Month::March => "March",
            Month::April => "April",
            Month::May => "May",
            Month::June => "June",
            Month::July => "July",
            Month::August => "August",
            Month::September => "September",
            Month::October => "October",
            Month::November => "November",
            Month::December => "December",
        }
    }

    /// Abbreviated English name (`Jan`..`Dec`), for `%b` / `%h` formatting.
    pub const fn short_name(self) -> &'static str {
        match self {
            Month::January => "Jan",
            Month::February => "Feb",
            Month::March => "Mar",
            Month::April => "Apr",
            Month::May => "May",
            Month::June => "Jun",
            Month::July => "Jul",
            Month::August => "Aug",
            Month::September => "Sep",
            Month::October => "Oct",
            Month::November => "Nov",
            Month::December => "Dec",
        }
    }

    /// Month for a 3-letter abbreviation (`Jan`..`Dec`), case-sensitive.
    pub fn from_short_name(name: &str) -> Option<Month> {
        match name {
            "Jan" => Some(Month::January),
            "Feb" => Some(Month::February),
            "Mar" => Some(Month::March),
            "Apr" => Some(Month::April),
            "May" => Some(Month::May),
            "Jun" => Some(Month::June),
            "Jul" => Some(Month::July),
            "Aug" => Some(Month::August),
            "Sep" => Some(Month::September),
            "Oct" => Some(Month::October),
            "Nov" => Some(Month::November),
            "Dec" => Some(Month::December),
            _ => None,
        }
    }
}

impl fmt::Display for Month {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Month {
    type Err = Error;

    fn from_str(s: &str) -> Result<Month> {
        Month::from_name(s).ok_or_else(|| Error::invalid("unknown month name"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_round_trip_broad_range() {
        // Every day over a ±200k-day window (≈ ±547 years).
        let mut day = -200_000i64;
        while day <= 200_000 {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day, "day {day}");
            day += 1;
        }
    }

    #[test]
    fn civil_round_trip_year_span() {
        // Every day over a 6000-year span exercises all leap combinations
        // including century and 400-year boundaries.
        for year in -3000..=3000 {
            for month in 1..=12u32 {
                let dim = days_in_month(year, month);
                for day in 1..=dim {
                    let z = days_from_civil(year, month, day);
                    assert_eq!(
                        civil_from_days(z),
                        (year, month, day),
                        "{year}-{month}-{day}"
                    );
                }
            }
        }
    }

    #[test]
    fn extreme_days_round_trip() {
        for day in [
            i32::MIN as i64,
            i32::MAX as i64,
            -719_468,
            0,
            719_468,
            2_000_000_000,
        ] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day, "day {day}");
        }
    }

    #[test]
    fn weekday_anchors() {
        // Known-anchor weekdays against the Unix epoch.
        assert_eq!(weekday_from_civil(0), Weekday::Thursday); // 1970-01-01
        assert_eq!(weekday_from_civil(10_957), Weekday::Saturday); // 2000-01-01
        assert_eq!(weekday_from_civil(18_628), Weekday::Friday); // 2021-01-01
        assert_eq!(weekday_from_civil(19_723), Weekday::Monday); // 2024-01-01
        assert_eq!(weekday_from_civil(-1), Weekday::Wednesday); // 1969-12-31
    }

    #[test]
    fn leap_year_rules() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(is_leap_year(0));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2100));
        assert!(!is_leap_year(2023));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 12), 31);
        assert_eq!(days_in_month(2024, 4), 30);
        assert_eq!(days_in_month(2024, 0), 0);
        assert_eq!(days_in_month(2024, 13), 0);
    }

    #[test]
    fn day_of_year_vectors() {
        assert_eq!(day_of_year(2023, 1, 1), 1);
        assert_eq!(day_of_year(2023, 12, 31), 365);
        assert_eq!(day_of_year(2024, 12, 31), 366);
        assert_eq!(day_of_year(2024, 3, 1), 61); // leap year
        assert_eq!(day_of_year(2023, 3, 1), 60);
    }

    #[test]
    fn iso_week_vectors() {
        // Reference values cross-checked against ISO 8601 tables.
        assert_eq!(iso_week_from_civil(2021, 1, 1), (2020, 53));
        assert_eq!(iso_week_from_civil(2021, 1, 3), (2020, 53));
        assert_eq!(iso_week_from_civil(2021, 1, 4), (2021, 1));
        assert_eq!(iso_week_from_civil(2021, 1, 10), (2021, 1));
        assert_eq!(iso_week_from_civil(2021, 1, 11), (2021, 2));
        assert_eq!(iso_week_from_civil(2020, 12, 31), (2020, 53));
        assert_eq!(iso_week_from_civil(2000, 1, 1), (1999, 52));
        assert_eq!(iso_week_from_civil(2016, 1, 1), (2015, 53));
        assert_eq!(iso_week_from_civil(2024, 1, 1), (2024, 1));
        assert_eq!(iso_week_from_civil(2024, 1, 7), (2024, 1));
        assert_eq!(iso_week_from_civil(2024, 1, 8), (2024, 2));
        assert_eq!(iso_week_from_civil(2025, 12, 31), (2026, 1));
        assert_eq!(iso_week_from_civil(1970, 1, 1), (1970, 1));
    }

    #[test]
    fn weekday_wraps() {
        assert_eq!(Weekday::Sunday.succ(), Weekday::Monday);
        assert_eq!(Weekday::Monday.pred(), Weekday::Sunday);
        assert_eq!(Weekday::Wednesday.succ(), Weekday::Thursday);
        assert_eq!(Weekday::from_iso_number(7), Some(Weekday::Sunday));
        assert_eq!(Weekday::from_iso_number(0), None);
        assert_eq!(Weekday::Thursday.number_from_monday(), 4);
        assert_eq!(Weekday::Sunday.number_from_sunday(), 1);
        assert_eq!(Weekday::Monday.number_from_sunday(), 2);
    }

    #[test]
    fn month_wraps_and_names() {
        assert_eq!(Month::December.next(), Month::January);
        assert_eq!(Month::January.prev(), Month::December);
        assert_eq!(Month::February.num_days(2024), 29);
        assert_eq!(Month::from_u32(13), None);
        assert_eq!(Month::September.as_u32(), 9);
        assert_eq!(Month::from_name("September"), Some(Month::September));
        assert_eq!(Month::from_name("september"), None);
    }

    #[test]
    fn week_of_year_never_out_of_bounds() {
        // No day in a broad window may report an ISO week outside 1..=53.
        let mut day = -200_000i64;
        while day <= 200_000 {
            let (y, m, d) = civil_from_days(day);
            let (_, week) = iso_week_from_civil(y, m, d);
            assert!((1..=53).contains(&week), "week {week} at {y}-{m}-{d}");
            day += 1;
        }
    }
}
