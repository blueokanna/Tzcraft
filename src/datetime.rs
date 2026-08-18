//! Civil date-time: [`CivilDateTime`].
//!
//! A zone-less calendar timestamp: a [`Date`] plus a [`TimeOfDay`]. It is
//! exactly the "wall clock" reading before any offset is applied, so it is
//! the natural type for parsing a local timestamp that carries no zone.
//!
//! Arithmetic is implemented by projecting onto the single `i128` nanosecond
//! timeline (day count × nanoseconds-per-day + nanoseconds-of-day), which
//! makes cross-midnight and cross-year carries a non-event.

use core::fmt;
use core::str::FromStr;

use crate::calendar::{ns_divmod_day, Month, Weekday, NS_PER_DAY};
use crate::date::Date;
use crate::duration::Duration;
use crate::error::{Error, Result};
use crate::format::{self, FractionDigits};
use crate::strftime;
use crate::ticks::Ticks;
use crate::time::TimeOfDay;
use crate::units::{Days, Months};
use crate::write::{with_buf, FmtSink, Write};

#[cfg(feature = "alloc")]
use alloc::string::String;

/// A calendar date and time without a timezone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDateTime {
    date: Date,
    time: TimeOfDay,
}

impl CivilDateTime {
    /// Build from parts.
    pub const fn new(date: Date, time: TimeOfDay) -> CivilDateTime {
        CivilDateTime { date, time }
    }

    /// Build from year/month/day/hour/minute/second.
    pub fn from_ymd_hms(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            Date::from_ymd(year, month, day)?,
            TimeOfDay::from_hms(hour, minute, second)?,
        ))
    }

    /// Build from year/month/day/hour/minute/second/millisecond.
    pub fn from_ymd_hms_milli(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        milli: u32,
    ) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            Date::from_ymd(year, month, day)?,
            TimeOfDay::from_hms_milli(hour, minute, second, milli)?,
        ))
    }

    /// Build from year/month/day/hour/minute/second/nanosecond.
    pub fn from_ymd_hms_nano(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        nano: u32,
    ) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            Date::from_ymd(year, month, day)?,
            TimeOfDay::from_hms_nano(hour, minute, second, nano)?,
        ))
    }

    /// The date part.
    #[inline]
    pub const fn date(self) -> Date {
        self.date
    }

    /// The time-of-day part.
    #[inline]
    pub const fn time(self) -> TimeOfDay {
        self.time
    }

    /// `(date, time)` parts.
    pub const fn parts(self) -> (Date, TimeOfDay) {
        (self.date, self.time)
    }

    /// Weekday of the date part.
    pub const fn weekday(self) -> Weekday {
        self.date.weekday()
    }

    /// 1-based day of year of the date part.
    pub const fn day_of_year(self) -> u32 {
        self.date.day_of_year()
    }

    /// ISO 8601 week date of the date part.
    pub const fn iso_week(self) -> crate::units::IsoWeek {
        self.date.iso_week()
    }

    /// Year.
    pub const fn year(self) -> i32 {
        self.date.year()
    }

    /// Month (1-based).
    pub const fn month(self) -> u32 {
        self.date.month()
    }

    /// Month as a [`Month`].
    pub fn month_enum(self) -> Month {
        self.date.month_enum()
    }

    /// Day of month.
    pub const fn day(self) -> u32 {
        self.date.day()
    }

    /// Hour.
    pub const fn hour(self) -> u32 {
        self.time.hour()
    }

    /// Minute.
    pub const fn minute(self) -> u32 {
        self.time.minute()
    }

    /// Second.
    pub const fn second(self) -> u32 {
        self.time.second()
    }

    /// Nanosecond within the second.
    pub const fn nanosecond(self) -> u32 {
        self.time.nanosecond()
    }

    /// Checked duration addition; carries across every boundary.
    pub fn checked_add(self, delta: Duration) -> Result<CivilDateTime> {
        let day_ns = self.date.days_since_epoch() as i128 * NS_PER_DAY;
        let total = day_ns
            .checked_add(self.time.nanos_since_midnight() as i128)
            .and_then(|t| t.checked_add(delta.as_nanos()))
            .ok_or_else(Error::overflow)?;
        let (days, rem) = ns_divmod_day(total);
        let days = i64::try_from(days).map_err(|_| Error::out_of_range("date"))?;
        let date = Date::from_days_checked(days)?;
        let time = TimeOfDay::from_nanos_since_midnight(rem as u64)?;
        Ok(CivilDateTime { date, time })
    }

    /// Checked duration subtraction.
    pub fn checked_sub(self, delta: Duration) -> Result<CivilDateTime> {
        self.checked_add(delta.checked_neg()?)
    }

    /// Alias for [`CivilDateTime::checked_add`] (chrono-compatible name).
    pub fn checked_add_signed(self, rhs: Duration) -> Result<CivilDateTime> {
        self.checked_add(rhs)
    }

    /// Alias for [`CivilDateTime::checked_sub`] (chrono-compatible name).
    pub fn checked_sub_signed(self, rhs: Duration) -> Result<CivilDateTime> {
        self.checked_sub(rhs)
    }

    /// The signed duration between `earlier` and `self`.
    pub fn signed_duration_since(self, earlier: CivilDateTime) -> Duration {
        let day_ns = (self.date.days_since_epoch() as i128
            - earlier.date.days_since_epoch() as i128)
            * NS_PER_DAY;
        let time_ns =
            self.time.nanos_since_midnight() as i128 - earlier.time.nanos_since_midnight() as i128;
        Duration::from_nanos(day_ns + time_ns)
    }

    /// Checked day offset; the time part is preserved.
    pub fn checked_add_days(self, days: Days) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date.checked_add_days(days)?,
            self.time,
        ))
    }

    /// Checked day offset in the negative direction.
    pub fn checked_sub_days(self, days: Days) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date.checked_sub_days(days)?,
            self.time,
        ))
    }

    /// Saturating duration addition.
    pub fn saturating_add(self, delta: Duration) -> CivilDateTime {
        match self.checked_add(delta) {
            Ok(dt) => dt,
            Err(_) => {
                if delta.as_nanos() >= 0 {
                    CivilDateTime::new(Date::MAX, TimeOfDay::MAX)
                } else {
                    CivilDateTime::new(Date::MIN, TimeOfDay::MIDNIGHT)
                }
            }
        }
    }

    /// Calendar-aware month stepping; the time part is preserved.
    pub fn checked_add_months(self, months: Months) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date.checked_add_months(months)?,
            self.time,
        ))
    }

    /// Calendar-aware month stepping in the negative direction.
    pub fn checked_sub_months(self, months: Months) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date.checked_sub_months(months)?,
            self.time,
        ))
    }

    /// Calendar-aware year stepping.
    pub fn checked_add_years(self, years: i32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date.checked_add_years(years)?,
            self.time,
        ))
    }

    /// Replace the year (fails for e.g. Feb 29 in a non-leap year).
    pub fn with_year(self, year: i32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(self.date.with_year(year)?, self.time))
    }

    /// Replace the month.
    pub fn with_month(self, month: u32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(self.date.with_month(month)?, self.time))
    }

    /// Replace the day.
    pub fn with_day(self, day: u32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(self.date.with_day(day)?, self.time))
    }

    /// Replace the hour.
    pub fn with_hour(self, hour: u32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(self.date, self.time.with_hour(hour)?))
    }

    /// Replace the minute.
    pub fn with_minute(self, minute: u32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date,
            self.time.with_minute(minute)?,
        ))
    }

    /// Replace the second.
    pub fn with_second(self, second: u32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date,
            self.time.with_second(second)?,
        ))
    }

    /// Replace the nanosecond within the second.
    pub fn with_nanosecond(self, nano: u32) -> Result<CivilDateTime> {
        Ok(CivilDateTime::new(
            self.date,
            self.time.with_nanosecond(nano)?,
        ))
    }

    /// Build from a Unix timestamp (chrono's `from_timestamp_opt`).
    pub fn from_timestamp(seconds: i64, nanos: u32) -> Result<CivilDateTime> {
        Ticks::from_timestamp(seconds, nanos)?.to_civil_utc()
    }

    /// Build from a Unix millisecond timestamp.
    pub fn from_timestamp_millis(millis: i64) -> Result<CivilDateTime> {
        Ticks::from_timestamp_millis(millis)?.to_civil_utc()
    }

    /// Build from a Unix microsecond timestamp.
    pub fn from_timestamp_micros(micros: i64) -> Result<CivilDateTime> {
        Ticks::from_timestamp_micros(micros)?.to_civil_utc()
    }

    /// Interpret this civil reading as UTC and project onto the timeline.
    pub fn to_ticks_utc(self) -> Result<Ticks> {
        let ns = self.date.days_since_epoch() as i128 * NS_PER_DAY
            + self.time.nanos_since_midnight() as i128;
        Ok(Ticks::from_unix_nanos(ns))
    }

    /// Strict ISO 8601 rendering (`YYYY-MM-DDTHH:MM:SS[.fffffffff]`).
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`CivilDateTime::write_iso`].
    #[cfg(feature = "alloc")]
    pub fn to_iso(self) -> String {
        format::format_civil(self, FractionDigits::Auto)
    }

    /// Strict ISO 8601 rendering into a caller-owned buffer (allocator-free).
    ///
    /// Returns the number of bytes written; a 32-byte buffer is always large
    /// enough.
    pub fn write_iso(self, out: &mut [u8]) -> Result<usize> {
        with_buf(out, |b| {
            format::format_date_into(b, self.year(), self.month(), self.day())?;
            b.write_byte(b'T')?;
            let (h, mi, s, ns) = self.time().parts();
            format::format_time_into(b, h, mi, s, ns, FractionDigits::Auto)
        })
    }

    /// Parse a strict ISO 8601 local date-time (no zone designator).
    pub fn from_iso(s: &str) -> Result<CivilDateTime> {
        let (date, time) = format::parse_civil_iso(s)?;
        Ok(CivilDateTime::new(date, time))
    }

    /// strftime-style rendering, e.g. `dt.format("%Y-%m-%d %H:%M:%S")`.
    ///
    /// The `%`-directive set (with `%-`/`%_`/`%0` padding modifiers) is
    /// documented on [`crate::Ticks::format`].
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`CivilDateTime::write_format`].
    #[cfg(feature = "alloc")]
    pub fn format(self, fmt: &str) -> Result<String> {
        strftime::format_civil(self, fmt)
    }

    /// strftime-style rendering into a caller-owned buffer (allocator-free).
    pub fn write_format(self, fmt: &str, out: &mut [u8]) -> Result<usize> {
        strftime::write_civil(self, fmt, out)
    }

    /// Parse with a strftime-style format string (chrono's
    /// `NaiveDateTime::parse_from_str`).
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<CivilDateTime> {
        strftime::parse_civil(fmt, s)
    }
}

impl fmt::Display for CivilDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sink = FmtSink(f);
        format::format_date_into(&mut sink, self.year(), self.month(), self.day())
            .and_then(|()| {
                sink.write_byte(b'T')?;
                let (h, mi, s, ns) = self.time().parts();
                format::format_time_into(&mut sink, h, mi, s, ns, FractionDigits::Auto)
            })
            .map_err(|_| fmt::Error)
    }
}

impl FromStr for CivilDateTime {
    type Err = Error;

    fn from_str(s: &str) -> Result<CivilDateTime> {
        CivilDateTime::from_iso(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_and_projection() {
        let dt = CivilDateTime::from_ymd_hms_nano(2024, 1, 2, 3, 4, 5, 6).unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 2);
        assert_eq!(dt.hour(), 3);
        assert_eq!(dt.minute(), 4);
        assert_eq!(dt.second(), 5);
        assert_eq!(dt.nanosecond(), 6);
        assert_eq!(dt.weekday(), Weekday::Tuesday);

        let t = dt.to_ticks_utc().unwrap();
        let back = t.to_civil_utc().unwrap();
        assert_eq!(back, dt);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn arithmetic_carries_across_days() {
        let dt = CivilDateTime::from_ymd_hms(2024, 12, 31, 23, 59, 59).unwrap();
        let next = dt.checked_add(Duration::from_seconds(2)).unwrap();
        assert_eq!(next.to_iso(), "2025-01-01T00:00:01");
        let back = dt.checked_sub(Duration::from_seconds(2)).unwrap();
        assert_eq!(back.to_iso(), "2024-12-31T23:59:57");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn months_keep_time() {
        let dt = CivilDateTime::from_ymd_hms(2024, 1, 31, 12, 30, 0).unwrap();
        let next = dt.checked_add_months(Months::new(1)).unwrap();
        assert_eq!(next.to_iso(), "2024-02-29T12:30:00");
        assert_eq!(next.time(), TimeOfDay::from_hms(12, 30, 0).unwrap());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn iso_round_trips() {
        for s in [
            "2024-01-01T00:00:00",
            "2024-02-29T23:59:59.123",
            "2024-06-15T08:30:00.123456789",
        ] {
            let dt = CivilDateTime::from_iso(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(dt.to_iso(), s, "{s}");
        }
        assert!(CivilDateTime::from_iso("2024-01-01T00:00:00Z").is_err());
        assert!(CivilDateTime::from_iso("2024-01-01").is_err());
    }
}
