//! Zoned instants: [`Zoned`].
//!
//! A `Zoned` is an instant on the timeline plus the zone it is read through:
//! `Ticks + Zone`, carried inline as a plain value. There is no hidden
//! context, no global "current zone", no mutable registry — the zone travels
//! with the instant, which makes `Zoned` trivially `Send`/`Sync`/`Copy` and
//! trivially serializable.
//!
//! The instant is always stored in UTC; the zone only changes how the civil
//! reading is projected. Month/year arithmetic is zone-aware: it operates on
//! the local civil reading and re-anchors, which is the behavior a calendar
//! user expects.

use core::fmt;
use core::str::FromStr;

use crate::date::Date;
use crate::datetime::CivilDateTime;
use crate::duration::Duration;
use crate::error::Result;
use crate::format::{self, FractionDigits};
use crate::offset::Offset;
use crate::strftime;
use crate::ticks::Ticks;
use crate::time::TimeOfDay;
use crate::units::{Days, Months};
use crate::write::{with_buf, write_signed_i128, FmtSink};
use crate::zone::Zone;

#[cfg(feature = "alloc")]
use alloc::string::String;

/// An instant on the timeline read through a specific [`Zone`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Zoned {
    ticks: Ticks,
    zone: Zone,
}

impl Zoned {
    /// Build from an instant and a zone (the instant is not shifted).
    pub fn new(ticks: Ticks, zone: Zone) -> Zoned {
        Zoned {
            ticks,
            zone: Zone::fixed(zone.offset()),
        }
    }

    /// Build from an instant carried as `Ticks`.
    pub fn from_ticks(ticks: Ticks, zone: Zone) -> Zoned {
        Zoned::new(ticks, zone)
    }

    /// Interpret a civil reading as local time in `zone` and anchor it.
    ///
    /// `12:00` local at `+08:00` becomes `04:00` UTC.
    pub fn from_civil(civil: CivilDateTime, zone: Zone) -> Result<Zoned> {
        let utc = civil.to_ticks_utc()?;
        let shifted = utc.checked_sub(Duration::from_seconds(zone.offset().as_seconds() as i64))?;
        Ok(Zoned {
            ticks: shifted,
            zone: Zone::fixed(zone.offset()),
        })
    }

    /// The underlying UTC instant.
    pub const fn ticks(self) -> Ticks {
        self.ticks
    }

    /// The zone.
    pub const fn zone(self) -> Zone {
        self.zone
    }

    /// The offset this zone currently applies.
    pub const fn offset(self) -> Offset {
        self.zone.offset()
    }

    /// Drop the zone and return the UTC instant.
    pub const fn to_utc(self) -> Ticks {
        self.ticks
    }

    /// The local civil reading (date + time) through this zone.
    pub fn civil(self) -> Result<CivilDateTime> {
        let shifted = self
            .ticks
            .checked_add(Duration::from_seconds(self.offset().as_seconds() as i64))?;
        shifted.to_civil_utc()
    }

    /// The local date.
    pub fn date(self) -> Result<Date> {
        Ok(self.civil()?.date())
    }

    /// The local time of day.
    pub fn time(self) -> Result<TimeOfDay> {
        Ok(self.civil()?.time())
    }

    /// Checked duration addition on the instant.
    pub fn checked_add(self, delta: Duration) -> Result<Zoned> {
        Ok(Zoned {
            ticks: self.ticks.checked_add(delta)?,
            zone: self.zone,
        })
    }

    /// Checked duration subtraction on the instant.
    pub fn checked_sub(self, delta: Duration) -> Result<Zoned> {
        self.checked_add(delta.checked_neg()?)
    }

    /// Alias for [`Zoned::checked_add`] (chrono-compatible name).
    pub fn checked_add_signed(self, rhs: Duration) -> Result<Zoned> {
        self.checked_add(rhs)
    }

    /// Alias for [`Zoned::checked_sub`] (chrono-compatible name).
    pub fn checked_sub_signed(self, rhs: Duration) -> Result<Zoned> {
        self.checked_sub(rhs)
    }

    /// The signed duration between `earlier` and `self`.
    pub fn signed_duration_since(self, earlier: Zoned) -> Duration {
        self.ticks.duration_since(earlier.ticks)
    }

    /// Checked day offset on the instant.
    pub fn checked_add_days(self, days: Days) -> Result<Zoned> {
        let days =
            i64::try_from(days.get()).map_err(|_| crate::error::Error::out_of_range("days"))?;
        self.checked_add(Duration::from_days(days))
    }

    /// Checked day offset in the negative direction.
    pub fn checked_sub_days(self, days: Days) -> Result<Zoned> {
        let days =
            i64::try_from(days.get()).map_err(|_| crate::error::Error::out_of_range("days"))?;
        self.checked_sub(Duration::from_days(days))
    }

    /// Zone-aware month stepping: adjust the local civil reading, re-anchor.
    pub fn checked_add_months(self, months: Months) -> Result<Zoned> {
        let civil = self.civil()?.checked_add_months(months)?;
        Zoned::from_civil(civil, self.zone)
    }

    /// Zone-aware month stepping in the negative direction.
    pub fn checked_sub_months(self, months: Months) -> Result<Zoned> {
        let civil = self.civil()?.checked_sub_months(months)?;
        Zoned::from_civil(civil, self.zone)
    }

    /// Zone-aware year stepping.
    pub fn checked_add_years(self, years: i32) -> Result<Zoned> {
        let civil = self.civil()?.checked_add_years(years)?;
        Zoned::from_civil(civil, self.zone)
    }

    /// Whole seconds since the epoch, flooring.
    pub fn timestamp(self) -> Result<i64> {
        self.ticks.timestamp()
    }

    /// Whole milliseconds since the epoch, flooring.
    pub fn timestamp_millis(self) -> Result<i64> {
        self.ticks.timestamp_millis()
    }

    /// Whole microseconds since the epoch, flooring.
    pub fn timestamp_micros(self) -> Result<i64> {
        self.ticks.timestamp_micros()
    }

    /// Nanoseconds since the epoch as `i64`.
    pub fn timestamp_nanos(self) -> Result<i64> {
        self.ticks.timestamp_nanos()
    }

    /// The current instant in UTC (requires the `std` feature).
    #[cfg(feature = "std")]
    pub fn now_utc() -> Result<Zoned> {
        Ok(Zoned::new(Ticks::now()?, Zone::Utc))
    }

    /// The current instant in UTC (requires the `std` feature).
    #[cfg(feature = "std")]
    pub fn now() -> Result<Zoned> {
        Zoned::now_utc()
    }

    /// Re-read the same instant through a different zone.
    pub fn with_zone(self, zone: Zone) -> Zoned {
        Zoned {
            ticks: self.ticks,
            zone: Zone::fixed(zone.offset()),
        }
    }

    /// RFC 3339 rendering with the local offset designator.
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Zoned::write_rfc3339`].
    #[cfg(feature = "alloc")]
    pub fn to_rfc3339(self, fraction: FractionDigits) -> String {
        match self.civil() {
            Ok(dt) => format::format_rfc3339_alloc(dt.date(), dt.time(), self.offset(), fraction)
                .expect("an RFC 3339 timestamp fits 64 bytes"),
            Err(_) => alloc::format!("{}{}", self.ticks.as_unix_nanos(), self.offset().to_iso()),
        }
    }

    /// RFC 3339 rendering into a caller-owned buffer (allocator-free).
    ///
    /// Returns the number of bytes written; a 64-byte buffer is always large
    /// enough.
    pub fn write_rfc3339(self, out: &mut [u8], fraction: FractionDigits) -> Result<usize> {
        match self.civil() {
            Ok(dt) => format::write_rfc3339(out, dt.date(), dt.time(), self.offset(), fraction),
            Err(_) => with_buf(out, |b| {
                write_signed_i128(b, self.ticks.as_unix_nanos())?;
                format::format_offset_into(b, self.offset())
            }),
        }
    }

    /// Parse a full RFC 3339 timestamp with an explicit offset.
    ///
    /// `Z` becomes a [`Zone::Utc`]; a numeric offset becomes the
    /// corresponding fixed zone (zero offsets normalize to UTC).
    pub fn from_rfc3339(s: &str) -> Result<Zoned> {
        let (date, time, offset) = format::parse_rfc3339(s)?;
        Zoned::from_civil(CivilDateTime::new(date, time), Zone::fixed(offset))
    }

    /// strftime-style rendering in local time, e.g.
    /// `z.format("%Y-%m-%d %H:%M:%S %:z")`.
    ///
    /// The `%`-directive set is documented on [`crate::Ticks::format`].
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Zoned::write_format`].
    #[cfg(feature = "alloc")]
    pub fn format(self, fmt: &str) -> Result<String> {
        strftime::format_zoned(self, fmt)
    }

    /// strftime-style rendering into a caller-owned buffer (allocator-free).
    pub fn write_format(self, fmt: &str, out: &mut [u8]) -> Result<usize> {
        strftime::write_zoned(self, fmt, out)
    }

    /// Parse with a strftime-style format string (chrono's
    /// `DateTime::parse_from_str`); the civil path requires a timezone offset.
    pub fn parse_from_str(s: &str, fmt: &str) -> Result<Zoned> {
        strftime::parse_zoned(fmt, s)
    }

    /// RFC 2822 rendering in local time (email / HTTP header dates).
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Zoned::write_rfc2822`].
    #[cfg(feature = "alloc")]
    pub fn to_rfc2822(self) -> String {
        match self.civil() {
            Ok(dt) => strftime::format_rfc2822(dt.date(), dt.time(), self.offset()),
            Err(_) => alloc::format!("{}{}", self.ticks.as_unix_nanos(), self.offset().to_iso()),
        }
    }

    /// RFC 2822 rendering into a caller-owned buffer (allocator-free).
    pub fn write_rfc2822(self, out: &mut [u8]) -> Result<usize> {
        match self.civil() {
            Ok(dt) => strftime::write_rfc2822(dt.date(), dt.time(), self.offset(), out),
            Err(_) => with_buf(out, |b| {
                write_signed_i128(b, self.ticks.as_unix_nanos())?;
                format::format_offset_into(b, self.offset())
            }),
        }
    }

    /// Parse an RFC 2822 date-time with its zone.
    pub fn from_rfc2822(s: &str) -> Result<Zoned> {
        let (date, time, offset) = strftime::parse_rfc2822(s)?;
        Zoned::from_civil(CivilDateTime::new(date, time), Zone::fixed(offset))
    }
}

impl fmt::Display for Zoned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.civil() {
            Ok(dt) => {
                let mut sink = FmtSink(f);
                format::format_rfc3339_into(
                    &mut sink,
                    dt.date(),
                    dt.time(),
                    self.offset(),
                    FractionDigits::Auto,
                )
                .map_err(|_| fmt::Error)
            }
            Err(_) => write!(f, "{}{}", self.ticks.as_unix_nanos(), self.offset()),
        }
    }
}

impl FromStr for Zoned {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> Result<Zoned> {
        Zoned::from_rfc3339(s)
    }
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use crate::error::Error;

    #[cfg(feature = "alloc")]
    #[test]
    fn civil_anchor_and_projection() {
        let zone = Zone::fixed(Offset::from_hms(8, 0, 0).unwrap());
        let civil = CivilDateTime::from_ymd_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let zoned = Zoned::from_civil(civil, zone).unwrap();
        // 12:00 +08:00 == 04:00 UTC.
        assert_eq!(
            zoned.to_utc().to_rfc3339(FractionDigits::None),
            "2024-06-15T04:00:00Z"
        );
        assert_eq!(zoned.civil().unwrap(), civil);
        assert_eq!(zoned.date().unwrap(), civil.date());
        assert_eq!(zoned.time().unwrap(), civil.time());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn zone_aware_month_math() {
        let zone = Zone::fixed(Offset::from_hms(-5, 0, 0).unwrap());
        let zoned = Zoned::from_civil(
            CivilDateTime::from_ymd_hms(2024, 1, 31, 12, 0, 0).unwrap(),
            zone,
        )
        .unwrap();
        let next = zoned.checked_add_months(Months::new(1)).unwrap();
        assert_eq!(next.civil().unwrap().to_iso(), "2024-02-29T12:00:00");
        assert_eq!(
            zoned
                .checked_sub_months(Months::new(1))
                .unwrap()
                .civil()
                .unwrap()
                .to_iso(),
            "2023-12-31T12:00:00"
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn strftime_round_trips() {
        let z = Zoned::from_rfc3339("2024-06-15T07:00:00+08:00").unwrap();
        assert_eq!(
            z.format("%Y-%m-%d %H:%M:%S %z").unwrap(),
            "2024-06-15 07:00:00 +0800"
        );
        assert_eq!(
            z.format("%A %e %B %Y %I:%M %p").unwrap(),
            "Saturday 15 June 2024 07:00 AM"
        );
        assert_eq!(
            Zoned::parse_from_str("2024-06-15 07:00:00 +0800", "%Y-%m-%d %H:%M:%S %z").unwrap(),
            z
        );
        assert_eq!(
            z.timestamp().unwrap(),
            Ticks::from_rfc3339("2024-06-14T23:00:00Z")
                .unwrap()
                .timestamp()
                .unwrap()
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn rfc3339_round_trips() {
        for s in [
            "2024-06-15T12:00:00Z",
            "2024-06-15T12:00:00+08:00",
            "2024-06-15T12:00:00.5-05:30",
            "2024-06-15T12:00:00-04:00",
        ] {
            let z = Zoned::from_rfc3339(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(z.to_rfc3339(FractionDigits::Auto), s, "{s}");
        }
        // A zero offset normalizes to UTC on the zone, but text stays explicit.
        let z = Zoned::from_rfc3339("2024-06-15T12:00:00+00:00").unwrap();
        assert_eq!(z.zone(), Zone::Utc);
        assert_eq!(z.to_rfc3339(FractionDigits::None), "2024-06-15T12:00:00Z");
        // Missing offset is rejected.
        assert_eq!(
            Zoned::from_rfc3339("2024-06-15T12:00:00"),
            Err(Error::parse("expected a timezone offset", 19))
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn with_zone_keeps_instant() {
        let z1 = Zoned::from_rfc3339("2024-06-15T12:00:00+08:00").unwrap();
        let z2 = z1.with_zone(Zone::Utc);
        assert_eq!(z1.to_utc(), z2.to_utc());
        assert_eq!(z2.to_rfc3339(FractionDigits::None), "2024-06-15T04:00:00Z");
    }
}
