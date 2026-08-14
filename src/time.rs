//! Civil time of day: [`TimeOfDay`].
//!
//! Nanoseconds since midnight, stored as a validated `u64` in
//! `0..86_400_000_000_000`. All accessors are arithmetic on that single
//! counter; there is no hour/minute/second state to desynchronize.

use alloc::string::String;
use core::fmt;
use core::str::FromStr;

use crate::calendar::NS_PER_DAY;
use crate::duration::Duration;
use crate::error::{Error, Result};
use crate::format::{self, FractionDigits};
use crate::strftime;

/// A time of day, nanosecond precision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOfDay(u64);

impl TimeOfDay {
    /// 00:00:00.000000000.
    pub const MIDNIGHT: TimeOfDay = TimeOfDay(0);

    /// 12:00:00.000000000.
    pub const NOON: TimeOfDay = TimeOfDay(NS_PER_DAY as u64 / 2);

    /// 23:59:59.999999999.
    pub const MAX: TimeOfDay = TimeOfDay(NS_PER_DAY as u64 - 1);

    /// Build from nanoseconds since midnight; validates the day bound.
    pub const fn from_nanos_since_midnight(nanos: u64) -> Result<TimeOfDay> {
        if nanos >= NS_PER_DAY as u64 {
            return Err(Error::out_of_range("time-of-day"));
        }
        Ok(TimeOfDay(nanos))
    }

    /// Build from hour/minute/second.
    pub fn from_hms(hour: u32, minute: u32, second: u32) -> Result<TimeOfDay> {
        Self::from_hms_nano(hour, minute, second, 0)
    }

    /// Build from hour/minute/second/millisecond.
    pub fn from_hms_milli(hour: u32, minute: u32, second: u32, milli: u32) -> Result<TimeOfDay> {
        let nano = milli.checked_mul(1_000_000).ok_or_else(Error::overflow)?;
        Self::from_hms_nano(hour, minute, second, nano)
    }

    /// Build from hour/minute/second/microsecond.
    pub fn from_hms_micro(hour: u32, minute: u32, second: u32, micro: u32) -> Result<TimeOfDay> {
        let nano = micro.checked_mul(1_000).ok_or_else(Error::overflow)?;
        Self::from_hms_nano(hour, minute, second, nano)
    }

    /// Build from hour/minute/second/nanosecond.
    pub fn from_hms_nano(hour: u32, minute: u32, second: u32, nano: u32) -> Result<TimeOfDay> {
        if hour >= 24 {
            return Err(Error::out_of_range("hour"));
        }
        if minute >= 60 {
            return Err(Error::out_of_range("minute"));
        }
        if second >= 60 {
            return Err(Error::out_of_range("second"));
        }
        if nano >= 1_000_000_000 {
            return Err(Error::out_of_range("nanosecond"));
        }
        let ns = hour as u64 * 3_600_000_000_000
            + minute as u64 * 60_000_000_000
            + second as u64 * 1_000_000_000
            + nano as u64;
        Ok(TimeOfDay(ns))
    }

    /// Nanoseconds since midnight.
    #[inline]
    pub const fn nanos_since_midnight(self) -> u64 {
        self.0
    }

    /// Whole seconds since midnight (chrono's `Timelike::num_seconds_from_midnight`).
    #[inline]
    pub const fn num_seconds_from_midnight(self) -> u32 {
        (self.0 / 1_000_000_000) as u32
    }

    /// Replace the hour.
    pub fn with_hour(self, hour: u32) -> Result<TimeOfDay> {
        TimeOfDay::from_hms_nano(hour, self.minute(), self.second(), self.nanosecond())
    }

    /// Replace the minute.
    pub fn with_minute(self, minute: u32) -> Result<TimeOfDay> {
        TimeOfDay::from_hms_nano(self.hour(), minute, self.second(), self.nanosecond())
    }

    /// Replace the second.
    pub fn with_second(self, second: u32) -> Result<TimeOfDay> {
        TimeOfDay::from_hms_nano(self.hour(), self.minute(), second, self.nanosecond())
    }

    /// Replace the nanosecond within the second.
    pub fn with_nanosecond(self, nano: u32) -> Result<TimeOfDay> {
        TimeOfDay::from_hms_nano(self.hour(), self.minute(), self.second(), nano)
    }

    /// Hour (0..24).
    #[inline]
    pub const fn hour(self) -> u32 {
        (self.0 / 3_600_000_000_000) as u32
    }

    /// Minute (0..60).
    #[inline]
    pub const fn minute(self) -> u32 {
        ((self.0 / 60_000_000_000) % 60) as u32
    }

    /// Second (0..60).
    #[inline]
    pub const fn second(self) -> u32 {
        ((self.0 / 1_000_000_000) % 60) as u32
    }

    /// Nanosecond within the second (0..1e9).
    #[inline]
    pub const fn nanosecond(self) -> u32 {
        (self.0 % 1_000_000_000) as u32
    }

    /// Millisecond within the second (0..1000).
    pub const fn millisecond(self) -> u32 {
        self.nanosecond() / 1_000_000
    }

    /// Microsecond within the second (0..1e6).
    pub const fn microsecond(self) -> u32 {
        self.nanosecond() / 1_000
    }

    /// `(hour, minute, second, nanosecond)`.
    pub const fn parts(self) -> (u32, u32, u32, u32) {
        (self.hour(), self.minute(), self.second(), self.nanosecond())
    }

    /// Checked addition that must stay inside the day.
    pub fn checked_add(self, delta: Duration) -> Result<TimeOfDay> {
        let total = (self.0 as i128)
            .checked_add(delta.as_nanos())
            .ok_or_else(Error::overflow)?;
        if !(0..NS_PER_DAY).contains(&total) {
            return Err(Error::out_of_range("time-of-day"));
        }
        Ok(TimeOfDay(total as u64))
    }

    /// Checked subtraction that must stay inside the day.
    pub fn checked_sub(self, delta: Duration) -> Result<TimeOfDay> {
        self.checked_add(delta.checked_neg()?)
    }

    /// Alias for [`TimeOfDay::checked_add`] (chrono-compatible name).
    pub fn checked_add_signed(self, rhs: Duration) -> Result<TimeOfDay> {
        self.checked_add(rhs)
    }

    /// Alias for [`TimeOfDay::checked_sub`] (chrono-compatible name).
    pub fn checked_sub_signed(self, rhs: Duration) -> Result<TimeOfDay> {
        self.checked_sub(rhs)
    }

    /// Addition with a whole-day carry instead of an error.
    ///
    /// Returns `(time_in_day, whole_days_carried)`. The arithmetic saturates
    /// and the carry clamps at `i64` bounds for absurdly large durations, so
    /// no input can overflow or panic.
    pub fn overflowing_add(self, delta: Duration) -> (TimeOfDay, i64) {
        let total = (self.0 as i128).saturating_add(delta.as_nanos());
        let days = total.div_euclid(NS_PER_DAY);
        let rem = total.rem_euclid(NS_PER_DAY);
        let carry = if days > i64::MAX as i128 {
            i64::MAX
        } else if days < i64::MIN as i128 {
            i64::MIN
        } else {
            days as i64
        };
        (TimeOfDay(rem as u64), carry)
    }

    /// Signed difference `self - earlier` (chrono-compatible name).
    pub fn signed_duration_since(self, earlier: TimeOfDay) -> Duration {
        self.duration_since(earlier)
    }

    /// Signed difference `self - earlier`.
    pub fn duration_since(self, earlier: TimeOfDay) -> Duration {
        Duration::from_nanos(self.0 as i128 - earlier.0 as i128)
    }

    /// Strict ISO 8601 rendering (`HH:MM:SS[.fffffffff]`).
    pub fn to_iso(self) -> String {
        format::format_time(self, FractionDigits::Auto)
    }

    /// Parse a strict ISO 8601 time of day (`HH:MM:SS`, optional fraction).
    pub fn from_iso(s: &str) -> Result<TimeOfDay> {
        format::parse_time_iso(s)
    }

    /// strftime-style rendering (see [`crate::strftime`]).
    pub fn format(self, fmt: &str) -> Result<String> {
        strftime::format_time(self, fmt)
    }

    /// Parse with a strftime-style format string (chrono's
    /// `NaiveTime::parse_from_str`).
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<TimeOfDay> {
        strftime::parse_time(fmt, s)
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_iso())
    }
}

impl FromStr for TimeOfDay {
    type Err = Error;

    fn from_str(s: &str) -> Result<TimeOfDay> {
        TimeOfDay::from_iso(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components() {
        let t = TimeOfDay::from_hms_nano(23, 59, 59, 123_456_789).unwrap();
        assert_eq!(
            (t.hour(), t.minute(), t.second(), t.nanosecond()),
            (23, 59, 59, 123_456_789)
        );
        assert_eq!(t.millisecond(), 123);
        assert_eq!(t.microsecond(), 123_456);
        assert_eq!(TimeOfDay::NOON.hour(), 12);
        assert_eq!(TimeOfDay::MAX.nanosecond(), 999_999_999);
    }

    #[test]
    fn validation() {
        assert!(TimeOfDay::from_hms(24, 0, 0).is_err());
        assert!(TimeOfDay::from_hms(0, 60, 0).is_err());
        assert!(TimeOfDay::from_hms(0, 0, 60).is_err());
        assert!(TimeOfDay::from_hms_nano(0, 0, 0, 1_000_000_000).is_err());
        assert!(TimeOfDay::from_nanos_since_midnight(NS_PER_DAY as u64).is_err());
        assert!(TimeOfDay::from_nanos_since_midnight(NS_PER_DAY as u64 - 1).is_ok());
    }

    #[test]
    fn arithmetic() {
        let t = TimeOfDay::from_hms(23, 30, 0).unwrap();
        // 23:30 + 30 min = 00:00 the next day: outside the day, so error.
        assert!(t.checked_add(Duration::from_minutes(30)).is_err());
        assert_eq!(
            t.checked_add(Duration::from_minutes(29)).unwrap(),
            TimeOfDay::from_hms(23, 59, 0).unwrap()
        );
        assert_eq!(
            t.checked_sub(Duration::from_minutes(31)).unwrap(),
            TimeOfDay::from_hms(22, 59, 0).unwrap()
        );
        let (t2, carry) = t.overflowing_add(Duration::from_hours(25));
        // 23:30 + 25 h = 00:30 two days later.
        assert_eq!(carry, 2);
        assert_eq!(t2, TimeOfDay::from_hms(0, 30, 0).unwrap());
        assert_eq!(
            TimeOfDay::from_hms(1, 0, 0).unwrap().duration_since(TimeOfDay::from_hms(0, 30, 0).unwrap()),
            Duration::from_minutes(30)
        );
    }

    #[test]
    fn iso_round_trips() {
        for s in ["00:00:00", "23:59:59", "12:00:00.5", "12:00:00.123456789"] {
            let t = TimeOfDay::from_iso(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(t.to_iso(), s, "{s}");
        }
        assert!(TimeOfDay::from_iso("24:00:00").is_err());
        assert!(TimeOfDay::from_iso("12:00").is_err());
        assert!(TimeOfDay::from_iso("12:00:00Z").is_err());
    }
}
