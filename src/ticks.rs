//! The canonical timeline: [`Ticks`].
//!
//! `Ticks` is the only type in `tzcraft` that carries instant arithmetic.
//! It is a single signed 128-bit nanosecond counter since the Unix epoch
//! (`1970-01-01T00:00:00Z`, proleptic Gregorian). The 128-bit width buys the
//! full nanosecond precision of modern clocks *and* a range of roughly
//! ±292 billion years — there is no second instant type, no overflow-collapse
//! strategy, no "small/large" split to remember.
//!
//! Civil types ([`crate::Date`], [`crate::TimeOfDay`],
//! [`crate::CivilDateTime`]) are pure projections of this timeline. They hold
//! no arithmetic of their own; calendar-aware operations
//! (months, years) are implemented once here as project → adjust → re-project.

use core::fmt;
use core::str::FromStr;

use crate::calendar::{floor_div_ns, floor_rem_ns, ns_divmod_day, Weekday, NS_PER_SEC};
use crate::date::Date;
use crate::datetime::CivilDateTime;
use crate::duration::Duration;
use crate::error::{Error, Result};
use crate::format::{self, FractionDigits};
use crate::offset::Offset;
use crate::strftime;
use crate::time::TimeOfDay;
use crate::units::{Days, Months};
use crate::write::{with_buf, write_signed_i128, FmtSink, Write};
use crate::zone::Zone;
use crate::zoned::Zoned;

#[cfg(feature = "alloc")]
use alloc::string::String;

/// A signed 128-bit nanosecond instant since `1970-01-01T00:00:00Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticks(i128);

impl Ticks {
    /// The Unix epoch itself: `1970-01-01T00:00:00Z`.
    pub const EPOCH: Ticks = Ticks(0);

    /// The smallest representable instant.
    pub const MIN: Ticks = Ticks(i128::MIN);

    /// The largest representable instant.
    pub const MAX: Ticks = Ticks(i128::MAX);

    /// Build an instant from raw nanoseconds since the Unix epoch.
    pub const fn from_unix_nanos(nanos: i128) -> Ticks {
        Ticks(nanos)
    }

    /// Build an instant from whole seconds plus sub-second nanoseconds.
    ///
    /// `nanos` must be in `0..1_000_000_000`.
    pub const fn from_unix_seconds(seconds: i64, nanos: u32) -> Result<Ticks> {
        if nanos >= 1_000_000_000 {
            return Err(Error::out_of_range("nanosecond"));
        }
        Ok(Ticks(seconds as i128 * NS_PER_SEC + nanos as i128))
    }

    /// Build from a Unix timestamp (chrono's `DateTime::from_timestamp`).
    ///
    /// `nanos` must be in `0..1_000_000_000`.
    pub fn from_timestamp(seconds: i64, nanos: u32) -> Result<Ticks> {
        Ticks::from_unix_seconds(seconds, nanos)
    }

    /// Build from a Unix millisecond timestamp.
    pub fn from_timestamp_millis(millis: i64) -> Result<Ticks> {
        let ns = millis as i128 * 1_000_000;
        Ok(Ticks::from_unix_nanos(ns))
    }

    /// Build from a Unix microsecond timestamp.
    pub fn from_timestamp_micros(micros: i64) -> Result<Ticks> {
        let ns = micros as i128 * 1_000;
        Ok(Ticks::from_unix_nanos(ns))
    }

    /// Build from a Unix nanosecond timestamp.
    pub const fn from_timestamp_nanos(nanos: i128) -> Ticks {
        Ticks::from_unix_nanos(nanos)
    }

    /// Whole seconds since the epoch, flooring (the mathematically correct
    /// Unix time; `chrono` truncates toward zero for pre-epoch instants).
    #[inline]
    pub fn timestamp(self) -> Result<i64> {
        let secs = floor_div_ns(self.0, NS_PER_SEC);
        i64::try_from(secs).map_err(|_| Error::out_of_range("instant"))
    }

    /// Whole milliseconds since the epoch, flooring.
    #[inline]
    pub fn timestamp_millis(self) -> Result<i64> {
        let ms = floor_div_ns(self.0, 1_000_000);
        i64::try_from(ms).map_err(|_| Error::out_of_range("instant"))
    }

    /// Whole microseconds since the epoch, flooring.
    #[inline]
    pub fn timestamp_micros(self) -> Result<i64> {
        let us = floor_div_ns(self.0, 1_000);
        i64::try_from(us).map_err(|_| Error::out_of_range("instant"))
    }

    /// Nanoseconds since the epoch as `i64`; fails beyond the `i64` range.
    #[inline]
    pub fn timestamp_nanos(self) -> Result<i64> {
        i64::try_from(self.0).map_err(|_| Error::out_of_range("instant"))
    }

    /// Raw nanoseconds since the Unix epoch.
    #[inline]
    pub const fn as_unix_nanos(self) -> i128 {
        self.0
    }

    /// Decompose into whole seconds (floor) and sub-second nanoseconds.
    ///
    /// The `(i64 seconds, u32 nanoseconds)` pair is the same shape that
    /// `chrono`, `time` and `rustix` build instants from — a pure numeric
    /// accessor, with no dependency on any of those crates. Fails only for
    /// instants whose day count exceeds `i64`, i.e. roughly outside ±2.9
    /// billion years.
    #[inline]
    pub fn to_unix_seconds(self) -> Result<(i64, u32)> {
        let secs = floor_div_ns(self.0, NS_PER_SEC);
        let nanos = floor_rem_ns(self.0, NS_PER_SEC);
        let secs = i64::try_from(secs).map_err(|_| Error::out_of_range("instant"))?;
        Ok((secs, nanos as u32))
    }

    /// POSIX-`timespec`-shaped decomposition: `(seconds, nanoseconds)` as
    /// signed 64-bit values (the same layout as `struct timespec` on Unix,
    /// and therefore the shape `rustix`'s `Timespec` uses).
    ///
    /// Fails only when the whole-second count exceeds `i64` (≈ ±292 billion
    /// years); the nanosecond component is always in `0..1_000_000_000`.
    #[inline]
    pub fn to_timespec(self) -> Result<(i64, i64)> {
        let (secs, nanos) = self.to_unix_seconds()?;
        Ok((secs, nanos as i64))
    }

    /// Build from a POSIX-`timespec`-shaped `(seconds, nanoseconds)` pair.
    ///
    /// `nanos` must be in `0..1_000_000_000`; any other value is rejected
    /// (POSIX `timespec` normalization is not silently applied).
    pub fn from_timespec(seconds: i64, nanos: i64) -> Result<Ticks> {
        if !(0..1_000_000_000).contains(&nanos) {
            return Err(Error::out_of_range("nanosecond"));
        }
        Ticks::from_unix_seconds(seconds, nanos as u32)
    }

    /// The current wall-clock instant (requires the `std` feature).
    #[cfg(feature = "std")]
    pub fn now() -> Result<Ticks> {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| Error::invalid("system clock precedes the unix epoch"))?;
        Ok(Ticks(d.as_nanos() as i128))
    }

    /// Convert to a `std::time::SystemTime` (requires the `std` feature).
    ///
    /// Fails for instants before the Unix epoch (unrepresentable).
    #[cfg(feature = "std")]
    pub fn to_std_time(self) -> Result<std::time::SystemTime> {
        let (secs, nanos) = self.to_unix_seconds()?;
        if secs < 0 {
            return Err(Error::out_of_range("instant (before 1970)"));
        }
        std::time::UNIX_EPOCH
            .checked_add(std::time::Duration::new(secs as u64, nanos))
            .ok_or_else(|| Error::out_of_range("system time"))
    }

    /// Checked addition of a signed duration.
    pub fn checked_add(self, delta: Duration) -> Result<Ticks> {
        self.0
            .checked_add(delta.as_nanos())
            .map(Ticks)
            .ok_or_else(Error::overflow)
    }

    /// Checked subtraction of a signed duration.
    pub fn checked_sub(self, delta: Duration) -> Result<Ticks> {
        self.0
            .checked_sub(delta.as_nanos())
            .map(Ticks)
            .ok_or_else(Error::overflow)
    }

    /// Alias for [`Ticks::checked_add`] (chrono-compatible name).
    pub fn checked_add_signed(self, rhs: Duration) -> Result<Ticks> {
        self.checked_add(rhs)
    }

    /// Alias for [`Ticks::checked_sub`] (chrono-compatible name).
    pub fn checked_sub_signed(self, rhs: Duration) -> Result<Ticks> {
        self.checked_sub(rhs)
    }

    /// Checked day offset.
    pub fn checked_add_days(self, days: Days) -> Result<Ticks> {
        let days = i64::try_from(days.get()).map_err(|_| Error::out_of_range("days"))?;
        self.checked_add(Duration::from_days(days))
    }

    /// Checked day offset in the negative direction.
    pub fn checked_sub_days(self, days: Days) -> Result<Ticks> {
        let days = i64::try_from(days.get()).map_err(|_| Error::out_of_range("days"))?;
        self.checked_sub(Duration::from_days(days))
    }

    /// Saturating addition of a signed duration.
    pub fn saturating_add(self, delta: Duration) -> Ticks {
        Ticks(self.0.saturating_add(delta.as_nanos()))
    }

    /// Saturating subtraction of a signed duration.
    pub fn saturating_sub(self, delta: Duration) -> Ticks {
        Ticks(self.0.saturating_sub(delta.as_nanos()))
    }

    /// The signed duration between `earlier` and `self`.
    ///
    /// Saturates at the representable boundary instead of overflowing:
    /// `MAX - MIN` would wrap in release and panic in debug.
    pub fn duration_since(self, earlier: Ticks) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }

    /// Calendar-aware month stepping on the UTC civil projection.
    ///
    /// The day is clamped to the end of the target month
    /// (`2023-01-31` + 1 month = `2023-02-28`).
    pub fn checked_add_months(self, months: Months) -> Result<Ticks> {
        let dt = self.to_civil_utc()?;
        let dt = dt.checked_add_months(months)?;
        dt.to_ticks_utc()
    }

    /// Calendar-aware month stepping in the negative direction.
    pub fn checked_sub_months(self, months: Months) -> Result<Ticks> {
        let dt = self.to_civil_utc()?;
        let dt = dt.checked_sub_months(months)?;
        dt.to_ticks_utc()
    }

    /// Calendar-aware year stepping (see [`Ticks::checked_add_months`]).
    pub fn checked_add_years(self, years: i32) -> Result<Ticks> {
        let dt = self.to_civil_utc()?;
        let dt = dt.checked_add_years(years)?;
        dt.to_ticks_utc()
    }

    /// The UTC civil projection `(date, time-of-day)`.
    ///
    /// Fails only for instants whose day count exceeds `i64`.
    #[inline]
    pub fn to_civil_utc(self) -> Result<CivilDateTime> {
        let (days, rem) = ns_divmod_day(self.0);
        let days = i64::try_from(days).map_err(|_| Error::out_of_range("instant"))?;
        let date = Date::from_days_checked(days)?;
        let time = TimeOfDay::from_nanos_since_midnight(rem as u64)?;
        Ok(CivilDateTime::new(date, time))
    }

    /// The UTC civil date.
    pub fn date_utc(self) -> Result<Date> {
        Ok(self.to_civil_utc()?.date())
    }

    /// The UTC civil time-of-day.
    pub fn time_utc(self) -> Result<TimeOfDay> {
        Ok(self.to_civil_utc()?.time())
    }

    /// The UTC weekday.
    pub fn weekday_utc(self) -> Result<Weekday> {
        Ok(self.to_civil_utc()?.weekday())
    }

    /// Attach a zone without shifting the instant.
    pub fn to_zoned(self, zone: Zone) -> Zoned {
        Zoned::new(self, zone)
    }

    /// RFC 3339 rendering with the requested fractional-second precision.
    ///
    /// The canonical form carries the `Z` designator. Instants beyond the
    /// civil `i64` day range (≈ ±2.9 billion years) fall back to a raw
    /// nanosecond count followed by `s`.
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Ticks::write_rfc3339`].
    #[cfg(feature = "alloc")]
    pub fn to_rfc3339(self, fraction: FractionDigits) -> String {
        match self.to_civil_utc() {
            Ok(dt) => format::format_rfc3339_alloc(dt.date(), dt.time(), Offset::UTC, fraction)
                .expect("an RFC 3339 timestamp fits 64 bytes"),
            Err(_) => alloc::format!("{}s", self.0),
        }
    }

    /// RFC 3339 rendering into a caller-owned buffer (allocator-free).
    ///
    /// Returns the number of bytes written. A 64-byte buffer is always large
    /// enough; [`Error::buffer_overflow`] is returned when it is not.
    pub fn write_rfc3339(self, out: &mut [u8], fraction: FractionDigits) -> Result<usize> {
        match self.to_civil_utc() {
            Ok(dt) => format::write_rfc3339(out, dt.date(), dt.time(), Offset::UTC, fraction),
            Err(_) => with_buf(out, |b| {
                write_signed_i128(b, self.0)?;
                b.write_byte(b's')
            }),
        }
    }

    /// Parse a full RFC 3339 timestamp, normalizing to UTC.
    ///
    /// Accepts `T`/`t`/space separators, fractional seconds of 1..9 digits,
    /// and offsets in `Z`, `±HH:MM`, `±HHMM` or `±HH` form.
    pub fn from_rfc3339(s: &str) -> Result<Ticks> {
        let (date, time, offset) = format::parse_rfc3339(s)?;
        let dt = CivilDateTime::new(date, time);
        let utc = dt.to_ticks_utc()?;
        utc.checked_sub(Duration::from_seconds(offset.as_seconds() as i64))
    }

    /// strftime-style rendering, e.g. `t.format("%Y-%m-%d %H:%M:%S %z")`.
    ///
    /// The civil parts are UTC and the offset designator is `Z`. Supported
    /// directives: `%Y %y %C %m %d %e %j %H %I %k %l %M %S %f %.f %.3f %p
    /// %P %a %A %b %h %B %G %g %V %u %w %U %W %z %:z %Z %s %F %D %x %R %T
    /// %X %r %+ %n %t %%`, plus the `%-`/`%_`/`%0` padding modifiers.
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Ticks::write_format`].
    #[cfg(feature = "alloc")]
    pub fn format(self, fmt: &str) -> Result<String> {
        strftime::format_ticks(self, fmt)
    }

    /// strftime-style rendering into a caller-owned buffer (allocator-free).
    pub fn write_format(self, fmt: &str, out: &mut [u8]) -> Result<usize> {
        strftime::write_ticks(self, fmt, out)
    }

    /// Parse with a strftime-style format string (chrono's
    /// `DateTime::parse_from_str`); the civil path requires a timezone offset.
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<Ticks> {
        strftime::parse_ticks(fmt, s)
    }

    /// RFC 2822 rendering in UTC (email / HTTP header dates).
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Ticks::write_rfc2822`].
    #[cfg(feature = "alloc")]
    pub fn to_rfc2822(self) -> String {
        match self.to_civil_utc() {
            Ok(dt) => strftime::format_rfc2822(dt.date(), dt.time(), Offset::UTC),
            Err(_) => alloc::format!("{} +0000", self.0),
        }
    }

    /// RFC 2822 rendering into a caller-owned buffer (allocator-free).
    pub fn write_rfc2822(self, out: &mut [u8]) -> Result<usize> {
        match self.to_civil_utc() {
            Ok(dt) => strftime::write_rfc2822(dt.date(), dt.time(), Offset::UTC, out),
            Err(_) => with_buf(out, |b| {
                write_signed_i128(b, self.0)?;
                b.write_str(" +0000")
            }),
        }
    }

    /// Parse an RFC 2822 date-time, normalizing to UTC.
    pub fn from_rfc2822(s: &str) -> Result<Ticks> {
        let (date, time, offset) = strftime::parse_rfc2822(s)?;
        let utc = CivilDateTime::new(date, time).to_ticks_utc()?;
        utc.checked_sub(Duration::from_seconds(offset.as_seconds() as i64))
    }
}

impl fmt::Display for Ticks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_civil_utc() {
            Ok(dt) => {
                let mut sink = FmtSink(f);
                format::format_rfc3339_into(
                    &mut sink,
                    dt.date(),
                    dt.time(),
                    Offset::UTC,
                    FractionDigits::Auto,
                )
                .map_err(|_| fmt::Error)
            }
            Err(_) => write!(f, "{}s", self.0),
        }
    }
}

impl FromStr for Ticks {
    type Err = Error;

    fn from_str(s: &str) -> Result<Ticks> {
        Ticks::from_rfc3339(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_parts() {
        let t = Ticks::EPOCH;
        assert_eq!(t.to_unix_seconds().unwrap(), (0, 0));
        let dt = t.to_civil_utc().unwrap();
        assert_eq!((dt.year(), dt.month(), dt.day()), (1970, 1, 1));
        assert_eq!(dt.hour(), 0);
    }

    #[test]
    fn add_duration_crosses_midnight() {
        let t = Ticks::from_unix_seconds(86_399, 500_000_000).unwrap();
        let next = t
            .checked_add(Duration::from_seconds(1))
            .unwrap()
            .to_civil_utc()
            .unwrap();
        assert_eq!((next.year(), next.month(), next.day()), (1970, 1, 2));
        assert_eq!(next.time().hour(), 0);
    }

    #[test]
    fn sub_seconds_floor() {
        let t = Ticks::from_unix_seconds(0, 0)
            .unwrap()
            .checked_sub(Duration::from_nanos(1))
            .unwrap();
        assert_eq!(t.to_unix_seconds().unwrap(), (-1, 999_999_999));
        let (y, m, d) = t.to_civil_utc().unwrap().date().parts();
        assert_eq!((y, m, d), (1969, 12, 31));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn calendar_months_clamp() {
        let jan31 = Ticks::from_rfc3339("2023-01-31T12:00:00Z").unwrap();
        let feb = jan31.checked_add_months(Months::new(1)).unwrap();
        assert_eq!(feb.to_rfc3339(FractionDigits::None), "2023-02-28T12:00:00Z");
        let leap = Ticks::from_rfc3339("2024-01-31T12:00:00Z").unwrap();
        assert_eq!(
            leap.checked_add_months(Months::new(1))
                .unwrap()
                .to_rfc3339(FractionDigits::None),
            "2024-02-29T12:00:00Z"
        );
        assert_eq!(
            jan31
                .checked_add_years(2)
                .unwrap()
                .to_rfc3339(FractionDigits::None),
            "2025-01-31T12:00:00Z"
        );
    }

    #[test]
    fn timestamp_helpers() {
        let t = Ticks::from_timestamp(1_700_000_000, 123_456_789).unwrap();
        assert_eq!(t.timestamp().unwrap(), 1_700_000_000);
        assert_eq!(t.timestamp_millis().unwrap(), 1_700_000_000_123);
        assert_eq!(t.timestamp_micros().unwrap(), 1_700_000_000_123_456);
        assert_eq!(t.timestamp_nanos().unwrap(), 1_700_000_000_123_456_789);
        // Round trips through coarser units lose sub-unit precision.
        assert_eq!(
            Ticks::from_timestamp_millis(1_700_000_000_123).unwrap(),
            Ticks::from_unix_nanos(1_700_000_000_123_000_000)
        );
        assert_eq!(
            Ticks::from_timestamp_micros(1_700_000_000_123_456).unwrap(),
            Ticks::from_unix_nanos(1_700_000_000_123_456_000)
        );
        assert_eq!(Ticks::from_timestamp_nanos(1_700_000_000_123_456_789), t);
        // Pre-epoch flooring: -0.5 s is timestamp -1, matching Unix time.
        let before = Ticks::EPOCH
            .checked_sub(Duration::from_millis(500))
            .unwrap();
        assert_eq!(before.timestamp().unwrap(), -1);
        assert!(Ticks::from_timestamp(0, 1_000_000_000).is_err());
    }

    #[test]
    fn rfc3339_round_trips() {
        use crate::write::Buf;
        for s in [
            "1970-01-01T00:00:00Z",
            "2024-02-29T23:59:59.5Z",
            "2024-02-29T23:59:59.123456789Z",
            "2024-06-15T08:30:00+08:00",
            "2024-06-15T00:30:00+08:00",
            "2024-06-14T20:30:00-04:00",
            "2024-06-15T08:30:00+0530",
            "2024-06-15T08:30:00+05",
            "1969-12-31T23:59:59Z",
            "2021-01-01t00:00:00z",
        ] {
            let t = Ticks::from_rfc3339(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            // A UTC instant re-renders with 'Z'; reconstruct the local form.
            let (date, time, off) = format::parse_rfc3339(s).unwrap();
            let shifted = t
                .checked_add(Duration::from_seconds(off.as_seconds() as i64))
                .unwrap();
            let mut storage_a = [0u8; 64];
            let mut storage_b = [0u8; 64];
            let mut expect = Buf::new(&mut storage_a);
            format::format_rfc3339_into(&mut expect, date, time, off, FractionDigits::Auto)
                .unwrap();
            let mut got = Buf::new(&mut storage_b);
            format::format_rfc3339_into(
                &mut got,
                shifted.to_civil_utc().unwrap().date(),
                shifted.to_civil_utc().unwrap().time(),
                off,
                FractionDigits::Auto,
            )
            .unwrap();
            assert_eq!(got.as_str(), expect.as_str(), "{s}");
        }
    }

    #[test]
    fn rfc3339_rejects_garbage() {
        for s in [
            "",
            "2024",
            "2024-01-01",
            "2024-01-01T12:00:00",
            "2024-13-01T00:00:00Z",
            "2024-01-32T00:00:00Z",
            "2024-01-01T24:00:00Z",
            "2024-01-01T12:00:00+24:00",
            "2024-01-01T12:00:00.1234567890Z",
            "2024-01-01T12:00:00Z trailing",
            "abcd-01-01T00:00:00Z",
        ] {
            assert!(Ticks::from_rfc3339(s).is_err(), "{s} should fail");
        }
    }

    #[test]
    fn duration_since_sign() {
        let a = Ticks::from_unix_seconds(10, 0).unwrap();
        let b = Ticks::from_unix_seconds(20, 0).unwrap();
        assert_eq!(b.duration_since(a), Duration::from_seconds(10));
        assert_eq!(a.duration_since(b), Duration::from_seconds(-10));
    }

    #[cfg(feature = "std")]
    #[test]
    fn now_is_sane() {
        let now = Ticks::now().unwrap();
        let year = now.to_civil_utc().unwrap().year();
        assert!((2000..=3000).contains(&year));
        // Platform SystemTime ranges are often much narrower than Ticks.
        // Conversion must return an error, never panic through `Add`.
        let _ = Ticks::MAX.to_std_time();
    }
}
