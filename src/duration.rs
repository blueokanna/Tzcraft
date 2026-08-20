//! Signed durations: [`Duration`].
//!
//! A `Duration` is a signed 128-bit nanosecond span. It is deliberately a
//! distinct type from [`crate::Ticks`]: an instant and a span are not the same
//! kind of thing, and the type system should not let you add two instants.
//! Internally both are `i128` nanoseconds, so conversions are free.

use core::fmt;
use core::str::FromStr;

use crate::calendar::{
    floor_div_ns, floor_rem_ns, NS_PER_DAY, NS_PER_HOUR, NS_PER_MIN, NS_PER_SEC,
};
use crate::error::{Error, Result};
use crate::format;
use crate::write::FmtSink;

#[cfg(feature = "alloc")]
use alloc::string::String;

/// A signed nanosecond duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(i128);

impl Duration {
    /// Smallest representable duration.
    pub const MIN: Duration = Duration(i128::MIN);

    /// Largest representable duration.
    pub const MAX: Duration = Duration(i128::MAX);

    /// Zero length.
    pub const ZERO: Duration = Duration(0);

    /// Build from nanoseconds.
    pub const fn from_nanos(nanos: i128) -> Duration {
        Duration(nanos)
    }

    /// Build from microseconds.
    ///
    /// Values outside the nanosecond range saturate at [`Duration::MIN`] or
    /// [`Duration::MAX`] instead of wrapping or panicking.
    pub const fn from_micros(micros: i128) -> Duration {
        Duration(micros.saturating_mul(1_000))
    }

    /// Build from microseconds, returning an error when scaling would
    /// overflow the nanosecond representation.
    pub fn checked_from_micros(micros: i128) -> Result<Duration> {
        micros
            .checked_mul(1_000)
            .map(Duration)
            .ok_or_else(Error::overflow)
    }

    /// Build from milliseconds.
    ///
    /// Values outside the nanosecond range saturate at [`Duration::MIN`] or
    /// [`Duration::MAX`] instead of wrapping or panicking.
    pub const fn from_millis(millis: i128) -> Duration {
        Duration(millis.saturating_mul(1_000_000))
    }

    /// Build from milliseconds, returning an error when scaling would
    /// overflow the nanosecond representation.
    pub fn checked_from_millis(millis: i128) -> Result<Duration> {
        millis
            .checked_mul(1_000_000)
            .map(Duration)
            .ok_or_else(Error::overflow)
    }

    /// Build from whole seconds.
    pub const fn from_seconds(seconds: i64) -> Duration {
        Duration(seconds as i128 * NS_PER_SEC)
    }

    /// Whole seconds (chrono-compatible constructor name).
    pub const fn seconds(seconds: i64) -> Duration {
        Duration::from_seconds(seconds)
    }

    /// Whole minutes (chrono-compatible constructor name).
    ///
    /// The product is computed in `i128`, so `i64::MAX` minutes cannot
    /// overflow (a `i64`-wide intermediate would wrap silently).
    pub const fn minutes(minutes: i64) -> Duration {
        Duration(minutes as i128 * NS_PER_MIN)
    }

    /// Whole hours (chrono-compatible constructor name).
    pub const fn hours(hours: i64) -> Duration {
        Duration(hours as i128 * NS_PER_HOUR)
    }

    /// Whole days (chrono-compatible constructor name).
    pub const fn days(days: i64) -> Duration {
        Duration(days as i128 * NS_PER_DAY)
    }

    /// Build from whole minutes.
    pub const fn from_minutes(minutes: i64) -> Duration {
        Duration(minutes as i128 * NS_PER_MIN)
    }

    /// Build from whole hours.
    pub const fn from_hours(hours: i64) -> Duration {
        Duration(hours as i128 * NS_PER_HOUR)
    }

    /// Build from whole days.
    pub const fn from_days(days: i64) -> Duration {
        Duration(days as i128 * NS_PER_DAY)
    }

    /// Build from whole weeks (chrono-compatible constructor).
    pub const fn weeks(weeks: i64) -> Duration {
        Duration(weeks as i128 * 7 * NS_PER_DAY)
    }

    /// Build from whole milliseconds (chrono-compatible constructor).
    pub const fn milliseconds(millis: i64) -> Duration {
        Duration::from_millis(millis as i128)
    }

    /// Build from whole microseconds (chrono-compatible constructor).
    pub const fn microseconds(micros: i64) -> Duration {
        Duration::from_micros(micros as i128)
    }

    /// Build from whole nanoseconds (chrono-compatible constructor).
    pub const fn nanoseconds(nanos: i64) -> Duration {
        Duration::from_nanos(nanos as i128)
    }

    /// The zero duration (chrono-compatible constructor).
    pub const fn zero() -> Duration {
        Duration::ZERO
    }

    /// Raw nanoseconds (signed).
    #[inline]
    pub const fn as_nanos(self) -> i128 {
        self.0
    }

    /// Truncated microseconds (toward zero).
    pub const fn as_micros(self) -> i128 {
        self.0 / 1_000
    }

    /// Truncated milliseconds (toward zero).
    pub const fn as_millis(self) -> i128 {
        self.0 / 1_000_000
    }

    /// Truncated whole seconds (toward zero).
    pub const fn as_seconds(self) -> i128 {
        self.0 / NS_PER_SEC
    }

    /// Whole seconds as `f64`.
    pub fn as_seconds_f64(self) -> f64 {
        self.0 as f64 / NS_PER_SEC as f64
    }

    /// Total nanoseconds as `f64`.
    pub fn as_f64(self) -> f64 {
        self.0 as f64
    }

    /// Alias for [`Duration::as_seconds_f64`] (chrono-compatible name).
    pub fn to_seconds_f64(self) -> f64 {
        self.as_seconds_f64()
    }

    /// Truncated whole seconds toward zero as `i64` (chrono-compatible
    /// `num_seconds`). Fails only beyond the `i64` second range
    /// (≈ ±292 billion years).
    pub fn num_seconds(self) -> Result<i64> {
        i64::try_from(self.as_seconds()).map_err(|_| Error::out_of_range("duration"))
    }

    /// Truncated whole milliseconds toward zero (chrono-compatible `num_milliseconds`).
    pub fn num_milliseconds(self) -> Result<i64> {
        i64::try_from(self.as_millis()).map_err(|_| Error::out_of_range("duration"))
    }

    /// Truncated whole microseconds toward zero (chrono-compatible `num_microseconds`).
    pub fn num_microseconds(self) -> Result<i64> {
        i64::try_from(self.as_micros()).map_err(|_| Error::out_of_range("duration"))
    }

    /// Whole nanoseconds as `i64` (chrono-compatible `num_nanoseconds`).
    ///
    /// Unlike the truncated `num_*` family, nanoseconds are exact; the only
    /// failure is a span wider than `i64`.
    pub fn num_nanoseconds(self) -> Result<i64> {
        i64::try_from(self.0).map_err(|_| Error::out_of_range("duration"))
    }

    /// Truncated whole minutes toward zero (chrono-compatible `num_minutes`).
    pub fn num_minutes(self) -> Result<i64> {
        i64::try_from(self.0 / NS_PER_MIN).map_err(|_| Error::out_of_range("duration"))
    }

    /// Truncated whole hours toward zero (chrono-compatible `num_hours`).
    pub fn num_hours(self) -> Result<i64> {
        i64::try_from(self.0 / NS_PER_HOUR).map_err(|_| Error::out_of_range("duration"))
    }

    /// Truncated whole days toward zero (chrono-compatible `num_days`).
    pub fn num_days(self) -> Result<i64> {
        i64::try_from(self.0 / NS_PER_DAY).map_err(|_| Error::out_of_range("duration"))
    }

    /// Truncated whole weeks toward zero (chrono-compatible `num_weeks`).
    pub fn num_weeks(self) -> Result<i64> {
        i64::try_from(self.0 / (7 * NS_PER_DAY)).map_err(|_| Error::out_of_range("duration"))
    }

    /// Whether this is exactly zero.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Whether this is strictly positive.
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// Whether this is strictly negative.
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Absolute value; fails on `i128::MIN`.
    pub fn checked_abs(self) -> Result<Duration> {
        if self.0 == i128::MIN {
            return Err(Error::overflow());
        }
        Ok(Duration(if self.0 < 0 { -self.0 } else { self.0 }))
    }

    /// Absolute value as an unsigned magnitude.
    pub const fn unsigned_abs(self) -> u128 {
        self.0.unsigned_abs()
    }

    /// Checked negation; fails on `i128::MIN`.
    pub fn checked_neg(self) -> Result<Duration> {
        self.0
            .checked_neg()
            .map(Duration)
            .ok_or_else(Error::overflow)
    }

    /// Checked addition.
    pub fn checked_add(self, rhs: Duration) -> Result<Duration> {
        self.0
            .checked_add(rhs.0)
            .map(Duration)
            .ok_or_else(Error::overflow)
    }

    /// Checked subtraction.
    pub fn checked_sub(self, rhs: Duration) -> Result<Duration> {
        self.0
            .checked_sub(rhs.0)
            .map(Duration)
            .ok_or_else(Error::overflow)
    }

    /// Checked scalar multiplication.
    pub fn checked_mul(self, rhs: i128) -> Result<Duration> {
        self.0
            .checked_mul(rhs)
            .map(Duration)
            .ok_or_else(Error::overflow)
    }

    /// Checked scalar division (truncates toward zero).
    pub fn checked_div(self, rhs: i128) -> Result<Duration> {
        self.0.checked_div(rhs).map(Duration).ok_or_else(|| {
            if rhs == 0 {
                Error::invalid("division by zero")
            } else {
                Error::overflow()
            }
        })
    }

    /// Checked duration ratio (truncates toward zero).
    pub fn checked_div_duration(self, rhs: Duration) -> Result<i128> {
        self.0.checked_div(rhs.0).ok_or_else(|| {
            if rhs.0 == 0 {
                Error::invalid("division by zero")
            } else {
                Error::overflow()
            }
        })
    }

    /// Saturating addition.
    pub fn saturating_add(self, rhs: Duration) -> Duration {
        Duration(self.0.saturating_add(rhs.0))
    }

    /// Saturating subtraction.
    pub fn saturating_sub(self, rhs: Duration) -> Duration {
        Duration(self.0.saturating_sub(rhs.0))
    }

    /// Saturating scalar multiplication (chrono-compatible `saturating_mul`).
    pub fn saturating_mul(self, rhs: i128) -> Duration {
        Duration(self.0.saturating_mul(rhs))
    }

    /// Build from an unsigned `core::time::Duration`.
    pub fn from_std(d: core::time::Duration) -> Duration {
        Duration(d.as_secs() as i128 * NS_PER_SEC + d.subsec_nanos() as i128)
    }

    /// Convert to an unsigned `core::time::Duration`; fails if negative or
    /// wider than `u64` seconds.
    pub fn to_std(self) -> Result<core::time::Duration> {
        if self.0 < 0 {
            return Err(Error::invalid("negative duration"));
        }
        let secs = floor_div_ns(self.0, NS_PER_SEC);
        let nanos = floor_rem_ns(self.0, NS_PER_SEC);
        let secs = u64::try_from(secs).map_err(|_| Error::out_of_range("duration"))?;
        Ok(core::time::Duration::new(secs, nanos as u32))
    }

    /// ISO 8601 duration rendering (`P1DT2H3M4.5S`, `-PT1S`, `PT0S`).
    ///
    /// Year and month components are never produced because their lengths are
    /// calendar-dependent and therefore not a property of a fixed span.
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Duration::write_iso8601`].
    #[cfg(feature = "alloc")]
    pub fn to_iso8601(self) -> String {
        format::format_duration_iso(self)
    }

    /// ISO 8601 duration rendering into a caller-owned buffer
    /// (allocator-free).
    ///
    /// Returns the number of bytes written; a 64-byte buffer is always large
    /// enough.
    pub fn write_iso8601(self, out: &mut [u8]) -> Result<usize> {
        format::write_duration_iso(out, self)
    }

    /// Parse an ISO 8601 duration.
    ///
    /// Accepts weeks (`P2W`), days, and `T`-prefixed hours/minutes/seconds
    /// with a fractional part on the final component. Years (`Y`) and months
    /// before `T` are rejected as calendar-ambiguous.
    pub fn from_iso8601(s: &str) -> Result<Duration> {
        format::parse_duration_iso(s)
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sink = FmtSink(f);
        format::format_duration_iso_into(&mut sink, *self).map_err(|_| fmt::Error)
    }
}

impl FromStr for Duration {
    type Err = Error;

    fn from_str(s: &str) -> Result<Duration> {
        Duration::from_iso8601(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        assert_eq!(Duration::from_minutes(2), Duration::from_seconds(120));
        assert_eq!(Duration::from_hours(1), Duration::from_seconds(3600));
        assert_eq!(Duration::from_days(1), Duration::from_seconds(86_400));
        assert_eq!(Duration::from_millis(1500).as_micros(), 1_500_000);
        assert_eq!(Duration::from_seconds(5).as_nanos(), 5_000_000_000);
        assert_eq!(Duration::from_nanos(-1).as_seconds(), 0); // truncates toward zero
    }

    #[test]
    fn constructors_never_overflow() {
        // The unit-scaled constructors must compute in `i128`: a maximal
        // `i64` argument must neither panic (debug) nor wrap (release).
        assert_eq!(
            Duration::from_days(i64::MAX).as_nanos(),
            i64::MAX as i128 * NS_PER_DAY
        );
        assert_eq!(
            Duration::from_minutes(i64::MAX).as_nanos(),
            i64::MAX as i128 * NS_PER_MIN
        );
        assert_eq!(
            Duration::from_hours(i64::MAX).as_nanos(),
            i64::MAX as i128 * NS_PER_HOUR
        );
        assert_eq!(
            Duration::weeks(i64::MAX).as_nanos(),
            i64::MAX as i128 * 7 * NS_PER_DAY
        );
        assert_eq!(
            Duration::minutes(i64::MAX),
            Duration::from_minutes(i64::MAX)
        );
        assert_eq!(Duration::hours(i64::MAX), Duration::from_hours(i64::MAX));
        assert_eq!(Duration::days(i64::MAX), Duration::from_days(i64::MAX));
        assert_eq!(Duration::from_days(-1).as_nanos(), -NS_PER_DAY);
        assert_eq!(Duration::from_minutes(-1).as_nanos(), -NS_PER_MIN);
        assert_eq!(Duration::from_micros(i128::MAX), Duration::MAX);
        assert_eq!(Duration::from_micros(i128::MIN), Duration::MIN);
        assert_eq!(Duration::from_millis(i128::MAX), Duration::MAX);
        assert_eq!(Duration::from_millis(i128::MIN), Duration::MIN);
        assert!(Duration::checked_from_micros(i128::MAX).is_err());
        assert!(Duration::checked_from_millis(i128::MIN).is_err());
    }

    #[test]
    fn checked_division_never_panics_at_integer_extremes() {
        assert!(Duration::MIN.checked_div(-1).is_err());
        assert!(Duration::MIN
            .checked_div_duration(Duration::from_nanos(-1))
            .is_err());
        assert!(Duration::from_nanos(1).checked_div(0).is_err());
        assert_eq!(
            Duration::from_seconds(10).checked_div(2).unwrap(),
            Duration::from_seconds(5)
        );
    }

    #[test]
    fn std_bridge() {
        let d = Duration::from_std(core::time::Duration::new(1, 500));
        assert_eq!(d, Duration::from_nanos(1_000_000_500));
        assert_eq!(d.to_std().unwrap(), core::time::Duration::new(1, 500));
        assert!(Duration::from_nanos(-1).to_std().is_err());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn iso_round_trips() {
        // Canonical texts that render identically.
        for text in [
            "PT0S",
            "PT1S",
            "PT1.5S",
            "PT0.5S",
            "PT1M30S",
            "PT1H",
            "P1D",
            "P1DT2H3M4.5S",
            "-PT1S",
            "P1DT0.25S",
        ] {
            let d = Duration::from_iso8601(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!(d.to_iso8601(), text, "{text}");
        }

        // Weeks parse but canonicalize to days.
        assert_eq!(
            Duration::from_iso8601("P2W").unwrap().as_nanos(),
            1_209_600_000_000_000
        );
        assert_eq!(Duration::from_iso8601("P2W").unwrap().to_iso8601(), "P14D");
        assert_eq!(Duration::from_iso8601("P1W").unwrap().to_iso8601(), "P7D");
    }

    #[test]
    fn iso_rejects_calendar_ambiguous() {
        for s in [
            "P1Y",
            "P1M",
            "P",
            "PT",
            "P1S",
            "P1DT",
            "P1DT5",
            "abc",
            "PT1H2D",
            "P1W1D",
            "P1WT1H",
            "P1D1D",
            "PT1S1H",
            "PT1H1H",
            "PT1.5H30M",
        ] {
            assert!(Duration::from_iso8601(s).is_err(), "{s} should fail");
        }
    }
}
