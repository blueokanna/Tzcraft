//! UTC offset: [`Offset`].
//!
//! An offset is a signed whole-second displacement from UTC, constrained to
//! the open interval `(-24h, 24h)`. It is the smallest timezone concept there
//! is: no transitions, no rules, just "how far is this clock from UTC".

use alloc::string::String;
use core::fmt;
use core::str::FromStr;

use crate::calendar::SECS_PER_DAY;
use crate::error::{Error, Result};
use crate::format;

/// A UTC offset in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset(i32);

impl Offset {
    /// The zero offset.
    pub const UTC: Offset = Offset(0);

    /// Build from a whole-second offset; `None` outside `(-24h, 24h)`.
    pub const fn from_seconds_opt(seconds: i32) -> Option<Offset> {
        if seconds <= -(SECS_PER_DAY as i32) || seconds >= SECS_PER_DAY as i32 {
            return None;
        }
        Some(Offset(seconds))
    }

    /// Build from a whole-second offset; fails outside `(-24h, 24h)`.
    pub const fn from_seconds(seconds: i32) -> Result<Offset> {
        match Self::from_seconds_opt(seconds) {
            Some(offset) => Ok(offset),
            None => Err(Error::invalid_offset()),
        }
    }

    /// Build from signed hours plus minutes and seconds.
    ///
    /// `hours` carries the sign and the whole offset shares it
    /// (`from_hms(-8, 30, 15)` is `-08:30:15`); minutes and seconds are
    /// magnitudes. The total must stay inside `(-24h, 24h)`.
    pub fn from_hms(hours: i32, minutes: u32, seconds: u32) -> Result<Offset> {
        if minutes >= 60 {
            return Err(Error::out_of_range("minute"));
        }
        if seconds >= 60 {
            return Err(Error::out_of_range("second"));
        }
        let sign = if hours < 0 { -1i64 } else { 1i64 };
        let magnitude = (hours as i64).abs() * 3600 + minutes as i64 * 60 + seconds as i64;
        let total = i32::try_from(sign * magnitude).map_err(|_| Error::invalid_offset())?;
        Self::from_seconds(total)
    }

    /// Whole-second displacement.
    pub const fn as_seconds(self) -> i32 {
        self.0
    }

    /// `(signed_hours, minutes, seconds)`.
    pub fn as_hms(self) -> (i32, u32, u32) {
        let abs = self.0.unsigned_abs();
        (
            self.0.signum() * (abs / 3600) as i32,
            (abs / 60) % 60,
            abs % 60,
        )
    }

    /// Whether this is exactly UTC.
    pub const fn is_utc(self) -> bool {
        self.0 == 0
    }

    /// The mirrored offset.
    pub const fn negate(self) -> Offset {
        Offset(-self.0)
    }

    /// ISO 8601 rendering: `Z`, or `±HH:MM` with `:SS` when seconds differ.
    pub fn to_iso(self) -> String {
        format::format_offset(self)
    }

    /// Parse `Z`, `±HH:MM`, `±HHMM`, `±HH` or `±HH:MM:SS`.
    pub fn from_iso(s: &str) -> Result<Offset> {
        format::parse_offset_iso(s)
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_iso())
    }
}

impl FromStr for Offset {
    type Err = Error;

    fn from_str(s: &str) -> Result<Offset> {
        Offset::from_iso(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_and_parts() {
        let o = Offset::from_hms(-8, 30, 15).unwrap();
        assert_eq!(o.as_seconds(), -(8 * 3600 + 30 * 60 + 15));
        assert_eq!(o.as_hms(), (-8, 30, 15));
        assert!(Offset::from_hms(24, 0, 0).is_err());
        assert!(Offset::from_hms(0, 60, 0).is_err());
        assert!(Offset::from_seconds(0).unwrap().is_utc());
        assert_eq!(Offset::from_seconds(86_399).unwrap().as_seconds(), 86_399);
        assert!(Offset::from_seconds(86_400).is_err());
        assert!(Offset::from_seconds(-86_400).is_err());
    }

    #[test]
    fn iso_round_trips() {
        for s in ["Z", "+08:00", "-05:30", "-04:30:15"] {
            let o = Offset::from_iso(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(o.to_iso(), s, "{s}");
        }
        // "+0530" and "+05" parse but canonicalize to the colon form on output.
        assert_eq!(Offset::from_iso("+0530").unwrap().as_seconds(), 5 * 3600 + 30 * 60);
        assert_eq!(Offset::from_iso("+05").unwrap().as_seconds(), 5 * 3600);
        // A zero offset parses but canonicalizes to "Z".
        assert_eq!(Offset::from_iso("+00:00").unwrap(), Offset::UTC);
        assert!(Offset::from_iso("+24:00").is_err());
        assert!(Offset::from_iso("abc").is_err());
        assert!(Offset::from_iso("+08:60").is_err());
    }
}
