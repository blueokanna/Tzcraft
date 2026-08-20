//! # tzcraft
//!
//! A date and time library that refuses the usual design axioms.
//!
//! ## The four axioms
//!
//! **1. One timeline.** [`Ticks`] is the only type that does instant
//! arithmetic: a signed 128-bit nanosecond counter since the Unix epoch.
//! That single width buys full nanosecond precision *and* a range of roughly
//! ±5.4×10^21 years, so there is no "small instant / large instant" split
//! and no overflow-collapse strategy to learn. [`Duration`] is a distinct
//! signed span type — you cannot add two instants, the type system says so.
//!
//! **2. Civil types are projections, not owners.** [`Date`],
//! [`TimeOfDay`] and [`CivilDateTime`] hold no arithmetic of their own; they
//! are pure projections of the timeline onto the proleptic Gregorian
//! calendar. Calendar-aware operations (months, years) exist once, as
//! project → adjust → re-project. There is no matrix of `Add` impls to
//! implement or misuse.
//!
//! **3. The compiler is the calendar.** Every civil computation — leap
//! rules, the day-count inversion, weekdays, ISO weeks — is a `const fn`,
//! so the compiler folds calendar math at compile time. Timezones are
//! `const` data too: a [`Zone`] is either UTC or a fixed offset, carried
//! inline with the instant in [`Zoned`]. There is no global registry, no
//! mutable "current zone", no IANA download, no hidden context.
//!
//! **4. The codec picks the wire shape.** Every type implements `nextjson`'s
//! format-neutral contracts exactly once. Human-readable codecs (JSON via
//! `nextjson`) see ISO 8601 / RFC 3339 text; binary codecs (`rustbinary`)
//! see compact integers. The same implementation, two shapes, zero feature
//! toggles. JSON stays readable; the binary profile stays small.
//!
//! ## Scope, stated plainly
//!
//! `tzcraft` is `#![no_std]`, `#![deny(unsafe_code)]`, with no dependencies
//! beyond `nextjson` (text/`serde`) and `rustbinary` (binary). With the
//! default features it links `alloc` for the `String`-returning formatting
//! methods and the codecs; with `--no-default-features` it builds **without
//! an allocator at all** — parsing, arithmetic, `Display`/`FromStr` and the
//! `write_*` buffer APIs all keep working. It does **not** ship an IANA
//! timezone database and does **not** pretend that fixed offsets are
//! daylight-saving rules. If a wall clock must follow real transitions,
//! resolve the offset with your own policy and hand the resulting
//! `Zone::Fixed` to the library. The seam is explicit on purpose.
//!
//! ## `chrono` replacement surface
//!
//! The everyday `chrono` API is covered: strftime-style `format` /
//! `parse_from_str` on every type, `Days` / `Months` / `IsoWeek` units,
//! `from_timestamp` / `timestamp(_millis/_micros/_nanos)`, `Duration`
//! constructors and `num_*` counts, `signed_duration_since`,
//! `checked_add_signed`, `with_year`/`with_month`/..., RFC 2822, and the
//! serde story via `nextjson` / `rustbinary`. Three deliberate differences:
//! `format()` returns `Result` (unknown directives are errors), timestamps
//! use floor semantics for pre-epoch instants, and `num_*` returns
//! `Result<i64>` instead of silently overflowing. `Local` (system-local
//! offset) is out of scope without platform FFI; supply the offset yourself
//! through `Zone::fixed`. The full mapping table lives in the README.
//!
//! ## Quick start
//!
//! The example below uses only allocator-free APIs (`write_*` buffer
//! rendering), so it runs in every configuration. The codec round-trip is
//! shown in the [`codec`] module.
//!
//! ```
//! use tzcraft::{Date, Duration, Months, Offset, Ticks, Weekday, Zone, Zoned};
//!
//! // One timeline, any reading.
//! let launch = Ticks::from_rfc3339("2024-06-15T08:30:00Z")?;
//! let local = launch.to_zoned(Zone::fixed(Offset::from_hms(8, 0, 0)?));
//!
//! // Buffer rendering: no allocator required.
//! let mut out = [0u8; 64];
//! let n = local.write_rfc3339(&mut out, tzcraft::FractionDigits::None)?;
//! assert_eq!(&out[..n], b"2024-06-15T16:30:00+08:00");
//! assert_eq!(local.date()?.weekday(), Weekday::Saturday);
//!
//! // Const calendar math: the compiler computes this at compile time.
//! const NEW_YEAR_2025: Date = Date::from_days_since_epoch(20_089);
//! const WEEKDAY: Weekday = NEW_YEAR_2025.weekday();
//! assert_eq!(WEEKDAY, Weekday::Wednesday);
//!
//! // Calendar-aware months clamp instead of overflowing.
//! let jan = Date::from_ymd(2023, 1, 31)?;
//! assert_eq!(jan.checked_add_months(Months::new(1))?, Date::from_ymd(2023, 2, 28)?);
//!
//! // Durations are signed and ISO 8601 round-trip cleanly.
//! let span = Duration::from_iso8601("P1DT2H3M4.5S")?;
//! let n = span.write_iso8601(&mut out)?;
//! assert_eq!(&out[..n], b"P1DT2H3M4.5S");
//! # Ok::<(), tzcraft::Error>(())
//! ```
//!
//! ## `chrono` migration in one glance
//!
//! ```
//! use tzcraft::{CivilDateTime, Date, Duration, Months, Ticks, Weekday};
//!
//! // `DateTime::from_timestamp(1_700_000_000, 0)` / `.timestamp_millis()`
//! let now_ms = Ticks::from_timestamp(1_700_000_000, 0)?.timestamp_millis()?;
//! assert_eq!(now_ms, 1_700_000_000_000);
//! // `NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()`
//! let d = Date::from_ymd(2024, 2, 29)?;
//! // `d.format("%A %d %B %Y")` (allocator-free: `write_format`)
//! let mut out = [0u8; 64];
//! let n = d.write_format("%A %d %B %Y", &mut out)?;
//! assert_eq!(&out[..n], b"Thursday 29 February 2024");
//! // `d.and_hms_opt(12, 0, 0).unwrap()`
//! let dt = d.and_hms(12, 0, 0)?;
//! // `dt.format("%Y-%m-%d %H:%M:%S")`
//! let n = dt.write_format("%Y-%m-%d %H:%M:%S", &mut out)?;
//! assert_eq!(&out[..n], b"2024-02-29 12:00:00");
//! // `d.checked_add_months(Months::new(1))`, `checked_add_days(Days::new(1))`
//! assert_eq!(d.checked_add_months(Months::new(1))?, Date::from_ymd(2024, 3, 29)?);
//! // `NaiveDateTime::parse_from_str`
//! assert_eq!(
//!     CivilDateTime::parse_from_str("2024-02-29 12:00:00", "%Y-%m-%d %H:%M:%S")?,
//!     dt
//! );
//! // `dt.signed_duration_since(...)`
//! assert_eq!(dt.signed_duration_since(dt), Duration::ZERO);
//! # Ok::<(), tzcraft::Error>(())
//! ```

#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tzcraft")]
// `doc_cfg` (which absorbed `doc_auto_cfg` in Rust 1.92) adds "Available on
// crate feature ..." badges on docs.rs (nightly); on stable it is inert.
#![cfg_attr(docsrs, feature(doc_cfg))]

// The crate is `#![no_std]` in every configuration. With the default
// features it additionally links `alloc` (for the `String`-returning
// formatting methods and the codecs); with `--no-default-features` it builds
// without an allocator entirely (parsing, arithmetic, `Display`/`FromStr`
// and the `write_*` buffer APIs all keep working).
#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod calendar;
#[cfg(feature = "serde")]
pub mod codec;
mod date;
mod datetime;
mod duration;
mod error;
mod format;
/// Migration guide: bringing `chrono` / `time` / `rustix` code in
/// (documentation only, no code).
pub mod migration;
mod offset;
mod strftime;
mod ticks;
mod time;
mod units;
mod zone;
mod zoned;

#[cfg(feature = "binary")]
pub mod binary;
/// Allocator-free output sinks ([`write::Buf`] / [`write::Write`]).
pub mod write;

pub use crate::calendar::{Month, Weekday};
pub use crate::date::Date;
pub use crate::datetime::CivilDateTime;
pub use crate::duration::Duration;
pub use crate::error::{Error, ErrorKind, Result};
pub use crate::format::FractionDigits;
pub use crate::offset::Offset;
pub use crate::ticks::Ticks;
pub use crate::time::TimeOfDay;
pub use crate::units::{Days, IsoWeek, Months};
pub use crate::zone::Zone;
pub use crate::zoned::Zoned;
