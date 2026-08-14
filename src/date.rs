//! Civil date: [`Date`].
//!
//! A `Date` is a pure projection of the timeline onto the proleptic
//! Gregorian calendar. It stores days since `1970-01-01` as an `i32`
//! (≈ ±5.8 million years), and every accessor re-derives year/month/day with
//! the const calendar engine. It deliberately carries no arithmetic of its
//! own beyond the calendar-aware month/year stepping; day stepping is a
//! simple `i32` offset because a civil day is exactly one day.

use alloc::string::String;
use core::fmt;
use core::str::FromStr;

use crate::calendar::{
    Month, Weekday, civil_from_days, day_of_year, days_from_civil, days_in_month, days_in_year,
    floor_div, floor_mod, is_leap_year, iso_week_from_civil, weekday_from_civil, DAYS_0001_TO_1970,
    NS_PER_DAY,
};
use crate::datetime::CivilDateTime;
use crate::duration::Duration;
use crate::error::{Error, Result};
use crate::format;
use crate::strftime;
#[cfg(feature = "std")]
use crate::ticks::Ticks;
use crate::time::TimeOfDay;
use crate::units::{Days, IsoWeek, Months};

/// A civil date in the proleptic Gregorian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i32);

impl Date {
    /// The earliest representable date.
    pub const MIN: Date = Date(i32::MIN);

    /// The latest representable date.
    pub const MAX: Date = Date(i32::MAX);

    /// Build directly from days since `1970-01-01`. Every `i32` is valid.
    pub const fn from_days_since_epoch(days: i32) -> Date {
        Date(days)
    }

    /// Days since `1970-01-01`.
    pub const fn days_since_epoch(self) -> i32 {
        self.0
    }

    /// Build from a day count that may live outside the `i32` range.
    pub(crate) fn from_days_checked(days: i64) -> Result<Date> {
        let d = i32::try_from(days).map_err(|_| Error::out_of_range("date"))?;
        Ok(Date(d))
    }

    /// Build a date from year/month/day, validating every component.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Date> {
        if !(1..=12).contains(&month) {
            return Err(Error::out_of_range("month"));
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(Error::out_of_range("day"));
        }
        let z = days_from_civil(year, month, day);
        let d = i32::try_from(z).map_err(|_| Error::out_of_range("date"))?;
        Ok(Date(d))
    }

    /// Parse a strict ISO 8601 calendar date (`YYYY-MM-DD`, with optional
    /// expanded-year sign).
    pub fn from_iso(s: &str) -> Result<Date> {
        format::parse_date_iso(s)
    }

    /// Build a date from a year and a 1-based day-of-year (chrono's
    /// `from_yo`).
    pub fn from_yo(year: i32, ordinal: u32) -> Result<Date> {
        if ordinal == 0 || ordinal > days_in_year(year) {
            return Err(Error::out_of_range("ordinal"));
        }
        let z = days_from_civil(year, 1, 1) + ordinal as i64 - 1;
        let d = i32::try_from(z).map_err(|_| Error::out_of_range("date"))?;
        Ok(Date(d))
    }

    /// Build a date from an ISO week date `(iso_year, week, weekday)`
    /// (chrono's `from_isoywd`).
    pub fn from_isoywd(year: i32, week: u32, weekday: Weekday) -> Result<Date> {
        let monday = IsoWeek::new(year, week).monday()?;
        monday.checked_add_days(Days::new(weekday as u64))
    }

    /// Build a date from days since the Common Era epoch `0001-01-01`
    /// (chrono's `from_num_days_from_ce`).
    pub fn from_num_days_from_ce(days: i64) -> Result<Date> {
        Date::from_days_checked(days - DAYS_0001_TO_1970)
    }

    /// Today's date in UTC (requires the `std` feature).
    #[cfg(feature = "std")]
    pub fn today() -> Result<Date> {
        Ok(Ticks::now()?.to_civil_utc()?.date())
    }

    /// `(year, month, day)` components.
    pub const fn parts(self) -> (i32, u32, u32) {
        civil_from_days(self.0 as i64)
    }

    /// Calendar year.
    pub const fn year(self) -> i32 {
        self.parts().0
    }

    /// Month as a 1-based number.
    pub const fn month(self) -> u32 {
        self.parts().1
    }

    /// Day of month.
    pub const fn day(self) -> u32 {
        self.parts().2
    }

    /// Month as a [`Month`] value.
    pub fn month_enum(self) -> Month {
        Month::from_u32(self.month()).expect("date always carries a valid month")
    }

    /// Weekday.
    pub const fn weekday(self) -> Weekday {
        weekday_from_civil(self.0 as i64)
    }

    /// 1-based day of year.
    pub const fn day_of_year(self) -> u32 {
        day_of_year(self.year(), self.month(), self.day())
    }

    /// Number of days in the current month.
    pub const fn days_in_month(self) -> u32 {
        days_in_month(self.year(), self.month())
    }

    /// Number of days in the current year.
    pub const fn days_in_year(self) -> u32 {
        days_in_year(self.year())
    }

    /// Whether the current year is a leap year.
    pub const fn is_leap_year(self) -> bool {
        is_leap_year(self.year())
    }

    /// ISO 8601 week date `(iso_year, week)`.
    pub const fn iso_week(self) -> IsoWeek {
        let (y, w) = iso_week_from_civil(self.year(), self.month(), self.day());
        IsoWeek::new(y, w)
    }

    /// 1-based day-of-year (chrono's `Datelike::ordinal`).
    pub const fn ordinal(self) -> u32 {
        self.day_of_year()
    }

    /// Days since the Common Era epoch `0001-01-01` (chrono's
    /// `Datelike::num_days_from_ce`).
    pub const fn num_days_from_ce(self) -> i64 {
        self.0 as i64 + DAYS_0001_TO_1970
    }

    /// Replace the year, keeping month and day (fails for e.g. Feb 29 in a
    /// non-leap year).
    pub fn with_year(self, year: i32) -> Result<Date> {
        Date::from_ymd(year, self.month(), self.day())
    }

    /// Replace the month, keeping year and day.
    pub fn with_month(self, month: u32) -> Result<Date> {
        Date::from_ymd(self.year(), month, self.day())
    }

    /// Replace the day, keeping year and month.
    pub fn with_day(self, day: u32) -> Result<Date> {
        Date::from_ymd(self.year(), self.month(), day)
    }

    /// Attach a time of day.
    pub const fn and_time(self, time: TimeOfDay) -> CivilDateTime {
        CivilDateTime::new(self, time)
    }

    /// Attach an hour/minute/second.
    pub fn and_hms(self, hour: u32, minute: u32, second: u32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(self, TimeOfDay::from_hms(hour, minute, second)?))
    }

    /// Attach an hour/minute/second/millisecond.
    pub fn and_hms_milli(
        self,
        hour: u32,
        minute: u32,
        second: u32,
        milli: u32,
    ) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self,
            TimeOfDay::from_hms_milli(hour, minute, second, milli)?,
        ))
    }

    /// Attach an hour/minute/second/microsecond.
    pub fn and_hms_micro(
        self,
        hour: u32,
        minute: u32,
        second: u32,
        micro: u32,
    ) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self,
            TimeOfDay::from_hms_micro(hour, minute, second, micro)?,
        ))
    }

    /// Attach an hour/minute/second/nanosecond.
    pub fn and_hms_nano(
        self,
        hour: u32,
        minute: u32,
        second: u32,
        nano: u32,
    ) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self,
            TimeOfDay::from_hms_nano(hour, minute, second, nano)?,
        ))
    }

    /// Checked day offset; fails only when the result leaves the `i32` range.
    pub fn checked_add_days(self, days: Days) -> Result<Date> {
        let sum = self.0 as i128 + days.get() as i128;
        let d = i32::try_from(sum).map_err(|_| Error::out_of_range("date"))?;
        Ok(Date(d))
    }

    /// Checked day offset in the negative direction.
    pub fn checked_sub_days(self, days: Days) -> Result<Date> {
        let sum = self.0 as i128 - days.get() as i128;
        let d = i32::try_from(sum).map_err(|_| Error::out_of_range("date"))?;
        Ok(Date(d))
    }

    /// Saturating day offset.
    pub fn saturating_add_days(self, days: i64) -> Date {
        let sum = self.0 as i128 + days as i128;
        if sum < i32::MIN as i128 {
            Date::MIN
        } else if sum > i32::MAX as i128 {
            Date::MAX
        } else {
            Date(sum as i32)
        }
    }

    /// Signed calendar-month stepping used by every public month method.
    fn checked_add_signed_months(self, months: i64) -> Result<Date> {
        let (y, m, d) = self.parts();
        let total = y as i64 * 12 + (m as i64 - 1) + months;
        let ny = floor_div(total, 12);
        let nm = floor_mod(total, 12) + 1;
        if ny < i32::MIN as i64 || ny > i32::MAX as i64 {
            return Err(Error::overflow());
        }
        let nd = core::cmp::min(d, days_in_month(ny as i32, nm as u32));
        Date::from_ymd(ny as i32, nm as u32, nd)
    }

    /// Calendar-aware month stepping; the day clamps to the target month's
    /// length (`2023-01-31` + 1 month = `2023-02-28`).
    pub fn checked_add_months(self, months: Months) -> Result<Date> {
        self.checked_add_signed_months(months.get() as i64)
    }

    /// Calendar-aware month stepping in the negative direction.
    pub fn checked_sub_months(self, months: Months) -> Result<Date> {
        self.checked_add_signed_months(-(months.get() as i64))
    }

    /// Calendar-aware year stepping (see [`Date::checked_add_months`]).
    pub fn checked_add_years(self, years: i32) -> Result<Date> {
        self.checked_add_signed_months(years as i64 * 12)
    }

    /// Saturating calendar-aware month stepping.
    pub fn saturating_add_months(self, months: Months) -> Date {
        self.checked_add_months(months).unwrap_or(Date::MAX)
    }

    /// Add a signed duration (whole-day precision, floor semantics).
    pub fn checked_add_signed(self, rhs: Duration) -> Result<Date> {
        let days = rhs.as_nanos().div_euclid(NS_PER_DAY);
        let days = i64::try_from(days).map_err(|_| Error::out_of_range("duration"))?;
        if days >= 0 {
            self.checked_add_days(Days::new(days as u64))
        } else {
            self.checked_sub_days(Days::new(days.unsigned_abs()))
        }
    }

    /// Subtract a signed duration (whole-day precision, floor semantics).
    pub fn checked_sub_signed(self, rhs: Duration) -> Result<Date> {
        self.checked_add_signed(rhs.checked_neg()?)
    }

    /// The signed duration between `earlier` and `self`, in whole days.
    pub fn signed_duration_since(self, earlier: Date) -> Duration {
        Duration::from_days(self.0 as i64 - earlier.0 as i64)
    }

    /// The next calendar day; fails at [`Date::MAX`].
    pub fn checked_succ(self) -> Result<Date> {
        self.checked_add_days(Days::new(1))
    }

    /// The previous calendar day; fails at [`Date::MIN`].
    pub fn checked_pred(self) -> Result<Date> {
        self.checked_sub_days(Days::new(1))
    }

    /// The next date with `weekday`, strictly after `self`.
    pub fn next_occurrence(self, weekday: Weekday) -> Result<Date> {
        let cur = self.weekday() as i32;
        let target = weekday as i32;
        let mut diff = target - cur;
        if diff <= 0 {
            diff += 7;
        }
        self.checked_add_days(Days::new(diff as u64))
    }

    /// The previous date with `weekday`, strictly before `self`.
    pub fn previous_occurrence(self, weekday: Weekday) -> Result<Date> {
        let cur = self.weekday() as i32;
        let target = weekday as i32;
        let mut diff = cur - target;
        if diff <= 0 {
            diff += 7;
        }
        self.checked_sub_days(Days::new(diff as u64))
    }

    /// The next date with `weekday`, including `self` if it already matches.
    pub fn next_or_same(self, weekday: Weekday) -> Result<Date> {
        let cur = self.weekday() as i32;
        let target = weekday as i32;
        let diff = (target - cur + 7) % 7;
        self.checked_add_days(Days::new(diff as u64))
    }

    /// The previous date with `weekday`, including `self` if it already matches.
    pub fn previous_or_same(self, weekday: Weekday) -> Result<Date> {
        let cur = self.weekday() as i32;
        let target = weekday as i32;
        let diff = (cur - target + 7) % 7;
        self.checked_sub_days(Days::new(diff as u64))
    }

    /// The first day of the current month.
    pub fn first_day_of_month(self) -> Date {
        let (y, m, _) = self.parts();
        Date(days_from_civil(y, m, 1) as i32)
    }

    /// The last day of the current month.
    pub fn last_day_of_month(self) -> Date {
        let (y, m, _) = self.parts();
        Date(days_from_civil(y, m, days_in_month(y, m)) as i32)
    }

    /// Strict ISO 8601 rendering (`YYYY-MM-DD`, expanded-year sign as needed).
    pub fn to_iso(self) -> String {
        format::format_date(self)
    }

    /// strftime-style rendering; unknown directives are an error (unlike
    /// `chrono`, which silently drops them).
    pub fn format(self, fmt: &str) -> Result<String> {
        strftime::format_date(self, fmt)
    }

    /// Parse with a strftime-style format string (chrono's
    /// `NaiveDate::parse_from_str`).
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<Date> {
        strftime::parse_date(fmt, s)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_iso())
    }
}

impl FromStr for Date {
    type Err = Error;

    fn from_str(s: &str) -> Result<Date> {
        Date::from_iso(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components() {
        let d = Date::from_ymd(2024, 2, 29).unwrap();
        assert_eq!((d.year(), d.month(), d.day()), (2024, 2, 29));
        assert_eq!(d.month_enum(), Month::February);
        assert_eq!(d.weekday(), Weekday::Thursday);
        assert_eq!(d.day_of_year(), 60);
        assert_eq!(d.days_in_month(), 29);
        assert_eq!(d.days_in_year(), 366);
        assert!(d.is_leap_year());
        assert_eq!(d.iso_week().parts(), (2024, 9));
        assert_eq!(d.ordinal(), 60);
        assert_eq!(d.num_days_from_ce(), 738_944);
    }

    #[test]
    fn alternative_constructors() {
        let d = Date::from_yo(2024, 60).unwrap();
        assert_eq!(d, Date::from_ymd(2024, 2, 29).unwrap());
        assert!(Date::from_yo(2024, 367).is_err());
        assert!(Date::from_yo(2023, 366).is_err());

        let iso = Date::from_isoywd(2021, 1, Weekday::Monday).unwrap();
        assert_eq!(iso, Date::from_ymd(2021, 1, 4).unwrap());
        let iso = Date::from_isoywd(2020, 53, Weekday::Friday).unwrap();
        assert_eq!(iso, Date::from_ymd(2021, 1, 1).unwrap());

        assert_eq!(
            Date::from_num_days_from_ce(0).unwrap(),
            Date::from_ymd(1, 1, 1).unwrap()
        );
        assert_eq!(
            Date::from_num_days_from_ce(719_162).unwrap(),
            Date::from_ymd(1970, 1, 1).unwrap()
        );
    }

    #[test]
    fn with_component_builders() {
        let d = Date::from_ymd(2024, 2, 29).unwrap();
        assert!(d.with_year(2023).is_err()); // Feb 29 does not exist in 2023
        assert_eq!(d.with_year(2028).unwrap(), Date::from_ymd(2028, 2, 29).unwrap());
        assert_eq!(d.with_month(3).unwrap(), Date::from_ymd(2024, 3, 29).unwrap());
        assert_eq!(d.with_day(1).unwrap(), Date::from_ymd(2024, 2, 1).unwrap());
        assert!(d.with_day(30).is_err());
    }

    #[test]
    fn validation() {
        assert!(Date::from_ymd(2024, 0, 1).is_err());
        assert!(Date::from_ymd(2024, 13, 1).is_err());
        assert!(Date::from_ymd(2023, 2, 29).is_err());
        assert!(Date::from_ymd(2024, 2, 30).is_err());
        assert!(Date::from_ymd(2024, 4, 31).is_err());
        assert!(Date::from_ymd(2024, 12, 32).is_err());
        assert!(Date::from_ymd(2024, 1, 0).is_err());
        assert!(Date::from_ymd(-400, 2, 29).is_ok()); // year 0 leap year
    }

    #[test]
    fn month_clamping() {
        let jan31 = Date::from_ymd(2023, 1, 31).unwrap();
        assert_eq!(
            jan31.checked_add_months(Months::new(1)).unwrap(),
            Date::from_ymd(2023, 2, 28).unwrap()
        );
        assert_eq!(
            jan31.checked_add_months(Months::new(2)).unwrap(),
            Date::from_ymd(2023, 3, 31).unwrap()
        );
        assert_eq!(
            jan31.checked_add_months(Months::new(13)).unwrap(),
            Date::from_ymd(2024, 2, 29).unwrap()
        );
        assert_eq!(
            jan31.checked_sub_months(Months::new(13)).unwrap(),
            Date::from_ymd(2021, 12, 31).unwrap()
        );
        let mar31 = Date::from_ymd(2023, 3, 31).unwrap();
        assert_eq!(
            mar31.checked_sub_months(Months::new(1)).unwrap(),
            Date::from_ymd(2023, 2, 28).unwrap()
        );
    }

    #[test]
    fn signed_duration_math() {
        let d = Date::from_ymd(2024, 1, 1).unwrap();
        assert_eq!(d.signed_duration_since(Date::from_ymd(2023, 12, 31).unwrap()), Duration::from_days(1));
        assert_eq!(
            d.checked_add_signed(Duration::from_days(10)).unwrap(),
            Date::from_ymd(2024, 1, 11).unwrap()
        );
        assert_eq!(
            d.checked_sub_signed(Duration::from_days(1)).unwrap(),
            Date::from_ymd(2023, 12, 31).unwrap()
        );
    }

    #[test]
    fn strftime_round_trips() {
        let d = Date::from_ymd(2024, 2, 29).unwrap();
        assert_eq!(d.format("%Y-%m-%d").unwrap(), "2024-02-29");
        assert_eq!(d.format("%a %b %d %Y").unwrap(), "Thu Feb 29 2024");
        assert_eq!(d.format("%j").unwrap(), "060");
        assert_eq!(d.format("%G-W%V").unwrap(), "2024-W09");
        assert_eq!(d.format("%u").unwrap(), "4");
        assert_eq!(d.format("%e %-d").unwrap(), "29 29");
        assert!(d.format("%Q").is_err());
        assert_eq!(Date::parse_from_str("2024-02-29", "%Y-%m-%d").unwrap(), d);
        assert_eq!(Date::parse_from_str("Thu Feb 29 2024", "%a %b %d %Y").unwrap(), d);
        assert_eq!(Date::parse_from_str("29.02.2024", "%d.%m.%Y").unwrap(), d);
        assert_eq!(Date::parse_from_str("24-060", "%y-%j").unwrap(), d);
        assert_eq!(
            Date::parse_from_str("2024-W09-4", "%G-W%V-%u").unwrap(),
            d
        );
        assert!(Date::parse_from_str("2024-02-30", "%Y-%m-%d").is_err());
        assert!(Date::parse_from_str("Mon Feb 29 2024", "%a %b %d %Y").is_err());
    }

    #[test]
    fn occurrence_math() {
        let d = Date::from_ymd(2024, 1, 1).unwrap(); // Monday
        assert_eq!(d.next_occurrence(Weekday::Monday).unwrap(), Date::from_ymd(2024, 1, 8).unwrap());
        assert_eq!(d.next_or_same(Weekday::Monday).unwrap(), d);
        assert_eq!(d.previous_occurrence(Weekday::Monday).unwrap(), Date::from_ymd(2023, 12, 25).unwrap());
        assert_eq!(d.next_occurrence(Weekday::Friday).unwrap(), Date::from_ymd(2024, 1, 5).unwrap());
        assert_eq!(d.previous_or_same(Weekday::Monday).unwrap(), d);
    }

    #[test]
    fn month_bounds() {
        let d = Date::from_ymd(2024, 2, 15).unwrap();
        assert_eq!(d.first_day_of_month(), Date::from_ymd(2024, 2, 1).unwrap());
        assert_eq!(d.last_day_of_month(), Date::from_ymd(2024, 2, 29).unwrap());
    }

    #[test]
    fn iso_round_trips() {
        for s in ["1970-01-01", "2024-02-29", "0000-01-01", "9999-12-31", "-0001-12-31"] {
            let d = Date::from_iso(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(d.to_iso(), s, "{s}");
        }
        assert!(Date::from_iso("2024-02-30").is_err());
        assert!(Date::from_iso("2024-1-01").is_err());
        assert!(Date::from_iso("2024-01-01T00:00:00").is_err());
        assert!(Date::from_iso("2024-01-01Z").is_err());
    }

    #[test]
    fn day_range_saturates() {
        let max = Date::MAX;
        assert!(max.checked_succ().is_err());
        assert_eq!(max.saturating_add_days(5), Date::MAX);
        assert!(Date::MIN.checked_pred().is_err());
    }
}
