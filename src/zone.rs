//! Timezones as data: [`Zone`].
//!
//! A `Zone` is one of two things and nothing else:
//!
//! - [`Zone::Utc`] — the canonical UTC zone;
//! - [`Zone::Fixed`] — a fixed [`Offset`] that never changes.
//!
//! That is the whole model, and it is a deliberate one. There is no global
//! registry to mutate, no IANA database to download, no hidden DST lookup.
//! Named zones are plain `const` values the caller defines, e.g.
//!
//! ```
//! # use tzcraft::{Offset, Zone};
//! const TOKYO: Zone = Zone::fixed(Offset::east(9 * 3600));
//! ```
//!
//! If a wall clock must follow actual daylight-saving rules, resolve the
//! offset with your own policy (or a future tzdb-backed variant of `Zone`)
//! and feed the resulting `Zone::Fixed` in. The seam is narrow and explicit,
//! which is the point.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::str::FromStr;

use crate::error::{Error, Result};
use crate::format;
use crate::offset::Offset;
use crate::write::{with_buf, FmtSink, Write};

#[cfg(feature = "alloc")]
use alloc::string::String;

/// A timezone: UTC or a fixed offset.
#[derive(Clone, Copy, Debug)]
pub enum Zone {
    /// The canonical UTC zone.
    Utc,
    /// A fixed offset that never changes.
    Fixed(Offset),
}

impl Zone {
    /// The UTC zone.
    pub const fn utc() -> Zone {
        Zone::Utc
    }

    /// A fixed-offset zone; a zero offset normalizes to [`Zone::Utc`] so that
    /// equality stays canonical.
    pub const fn fixed(offset: Offset) -> Zone {
        if offset.is_utc() {
            Zone::Utc
        } else {
            Zone::Fixed(offset)
        }
    }

    /// Look up a tiny built-in alias table of unambiguous zone names.
    ///
    /// Only names that cannot be confused across conventions live here:
    /// `UTC`, `GMT`, `Z`, `Etc/UTC` and `Etc/GMT`. Everything else is a
    /// caller-defined `const` value.
    pub fn from_name(name: &str) -> Option<Zone> {
        match name {
            "UTC" | "GMT" | "Z" | "Etc/UTC" | "Etc/GMT" => Some(Zone::Utc),
            _ => None,
        }
    }

    /// The offset this zone applies at any instant (fixed by construction).
    pub const fn offset(self) -> Offset {
        match self {
            Zone::Utc => Offset::UTC,
            Zone::Fixed(offset) => offset,
        }
    }

    /// Whether this zone has a zero UTC offset.
    pub const fn is_utc(self) -> bool {
        self.offset().is_utc()
    }

    /// ISO 8601 rendering: `UTC` or the offset string.
    ///
    /// This method allocates. The allocator-free equivalent is
    /// [`Zone::write_iso`].
    #[cfg(feature = "alloc")]
    pub fn to_iso(self) -> String {
        if self.is_utc() {
            alloc::string::String::from("UTC")
        } else {
            format::format_offset(self.offset())
        }
    }

    /// ISO 8601 rendering into a caller-owned buffer (allocator-free).
    ///
    /// Returns the number of bytes written; a 10-byte buffer is always large
    /// enough.
    pub fn write_iso(self, out: &mut [u8]) -> Result<usize> {
        with_buf(out, |b| {
            if self.is_utc() {
                b.write_str("UTC")
            } else {
                format::format_offset_into(b, self.offset())
            }
        })
    }

    /// Parse `UTC`, `Z` or any offset form accepted by [`Offset::from_iso`].
    pub fn from_iso(s: &str) -> Result<Zone> {
        match s {
            "UTC" | "Z" | "z" => Ok(Zone::Utc),
            _ => Ok(Zone::fixed(Offset::from_iso(s)?)),
        }
    }
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_utc() {
            f.write_str("UTC")
        } else {
            let mut sink = FmtSink(f);
            format::format_offset_into(&mut sink, self.offset()).map_err(|_| fmt::Error)
        }
    }
}

impl PartialEq for Zone {
    fn eq(&self, other: &Zone) -> bool {
        self.offset() == other.offset()
    }
}

impl Eq for Zone {}

impl Hash for Zone {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.offset().hash(state);
    }
}

impl FromStr for Zone {
    type Err = Error;

    fn from_str(s: &str) -> Result<Zone> {
        Zone::from_iso(s)
    }
}

impl PartialOrd for Zone {
    fn partial_cmp(&self, other: &Zone) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Zone {
    /// Zones order by their offset, so `-08:00 < UTC < +08:00`.
    fn cmp(&self, other: &Zone) -> Ordering {
        self.offset().cmp(&other.offset())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_and_equality() {
        assert_eq!(Zone::fixed(Offset::UTC), Zone::Utc);
        assert_eq!(Zone::Fixed(Offset::UTC), Zone::Utc);
        assert_eq!(Zone::from_name("UTC"), Some(Zone::Utc));
        assert_eq!(Zone::from_name("CST"), None);
        let z = Zone::fixed(Offset::from_hms(8, 0, 0).unwrap());
        assert_eq!(z.offset().as_seconds(), 8 * 3600);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn iso_round_trips() {
        for s in ["UTC", "+08:00", "-05:30"] {
            let z = Zone::from_iso(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(z.to_iso(), s, "{s}");
        }
        // "Z" parses as UTC but canonicalizes to "UTC" on output.
        assert_eq!(Zone::from_iso("Z").unwrap(), Zone::Utc);
        assert_eq!(Zone::Fixed(Offset::UTC).to_iso(), "UTC");
        assert!(Zone::from_iso("+24:00").is_err());
    }

    #[test]
    fn ordering() {
        assert!(Zone::Utc < Zone::fixed(Offset::from_hms(1, 0, 0).unwrap()));
        assert!(Zone::fixed(Offset::from_hms(-1, 0, 0).unwrap()) < Zone::Utc);
    }
}
