//! Migration guide: bringing `chrono` / `time` / `rustix` code in.
//!
//! This module is **documentation only** — it contains no code and links no
//! third-party date/time crate. Migration is deliberately **one-way easy:
//! into `tzcraft`**. The crate's dependency graph contains no `chrono`,
//! `time` or `rustix` — only `nextjson` and `rustbinary` (both optional) —
//! so there is nothing here that links those crates and nothing that makes
//! leaving easier than arriving.
//!
//! # Moving into tzcraft (from `chrono`)
//!
//! The everyday `chrono` surface maps 1:1. `_opt` variants become plain `?`
//! on tzcraft's [`Result`](crate::Result) model.
//!
//! | `chrono` | `tzcraft` |
//! | --- | --- |
//! | `Utc::now()` / `Local::now()` | [`Ticks::now()`](crate::Ticks::now) / [`Zoned::now_utc()`](crate::Zoned::now_utc) |
//! | `DateTime::<Utc>` | [`Ticks`](crate::Ticks) |
//! | `DateTime::<FixedOffset>` | [`Zoned`](crate::Zoned) |
//! | `NaiveDate` | [`Date`](crate::Date) |
//! | `NaiveTime` | [`TimeOfDay`](crate::TimeOfDay) |
//! | `NaiveDateTime` | [`CivilDateTime`](crate::CivilDateTime) |
//! | `Duration` / `TimeDelta` | [`Duration`](crate::Duration) |
//! | `from_ymd_opt` / `from_hms_opt` | [`Date::from_ymd`](crate::Date::from_ymd) / [`TimeOfDay::from_hms`](crate::TimeOfDay::from_hms) |
//! | `DateTime::from_timestamp` / `timestamp()` | [`Ticks::from_timestamp`](crate::Ticks::from_timestamp) / [`Ticks::timestamp`](crate::Ticks::timestamp) (plus `_millis` / `_micros` / `_nanos`) |
//! | `date.and_hms_opt(...)` | `date.and_hms(...)?` |
//! | `d.checked_add_months(Months::new(1))` | same name, same signature |
//! | `d.checked_add_days(Days::new(1))` | same name, same signature |
//! | `dt.format(...)` | `dt.format(...)?` |
//! | `NaiveDate::parse_from_str` | [`Date::parse_from_str`](crate::Date::parse_from_str) |
//! | `NaiveDateTime::parse_from_str` | [`CivilDateTime::parse_from_str`](crate::CivilDateTime::parse_from_str) |
//! | `DateTime::parse_from_str` | [`Ticks::parse_from_str`](crate::Ticks::parse_from_str) / [`Zoned::parse_from_str`](crate::Zoned::parse_from_str) |
//! | `to_rfc3339` / `parse_from_rfc3339` | `to_rfc3339(frac)` / [`from_rfc3339`](crate::Ticks::from_rfc3339) |
//! | `to_rfc2822` / `parse_from_rfc2822` | same names |
//! | `Datelike::year/month/day/ordinal/weekday/iso_week/num_days_from_ce` | same-named inherent methods |
//! | `Timelike::hour/minute/second/nanosecond/num_seconds_from_midnight` | same-named inherent methods |
//! | `FixedOffset::east_opt` / `from_hms_opt` | [`Offset::from_seconds`](crate::Offset::from_seconds) / [`Offset::from_hms`](crate::Offset::from_hms) |
//! | `dt.with_timezone(...)` | [`Zoned::with_zone`](crate::Zoned::with_zone) |
//! | `checked_add_signed` / `signed_duration_since` | same names |
//! | `with_year/with_month/.../with_nanosecond` | same names |
//! | `Duration::to_std/from_std` | same names |
//!
//! Three deliberate differences — correctness or safety calls, not
//! oversights:
//!
//! 1. **`format()` returns [`Result`](crate::Result)`** — unknown directives
//!    are errors instead of being silently dropped.
//! 2. **`timestamp()` floors** — a pre-epoch instant like
//!    `1969-12-31T23:59:59.5Z` maps to `-1`, which is what Unix time means;
//!    `chrono` truncates toward zero and would give `0`.
//! 3. **`num_*` returns `Result<i64>`** — out-of-range values are explicit
//!    errors instead of silent overflow.
//!
//! `Local` (the system-local zone) is not in v1: pure `std` cannot read the
//! local offset (that needs `libc`/platform FFI, and the core crate only
//! depends on `nextjson` and `rustbinary` while denying `unsafe`). Resolve
//! the offset with a platform API and hand it to
//! [`Zone::fixed`](crate::Zone::fixed) — the seam is explicit.
//!
//! # Moving into tzcraft (from `time`)
//!
//! | `time` | `tzcraft` |
//! | --- | --- |
//! | `OffsetDateTime` | [`Zoned`](crate::Zoned) (or [`Ticks`](crate::Ticks) for UTC) |
//! | `PrimitiveDateTime` | [`CivilDateTime`](crate::CivilDateTime) |
//! | `Date` | [`Date`](crate::Date) |
//! | `Time` | [`TimeOfDay`](crate::TimeOfDay) |
//! | `UtcOffset` | [`Offset`](crate::Offset) |
//! | `Duration` | [`Duration`](crate::Duration) |
//! | `Weekday` / `Month` | [`Weekday`](crate::Weekday) / [`Month`](crate::Month) |
//!
//! `OffsetDateTime::unix_timestamp()` + `.nanosecond()` feed
//! [`Ticks::from_timestamp`](crate::Ticks::from_timestamp); `Date::year /
//! month / day` and `Time::hour / minute / second / nanosecond` feed
//! [`Date::from_ymd`](crate::Date::from_ymd) and
//! [`TimeOfDay::from_hms_nano`](crate::TimeOfDay::from_hms_nano).
//!
//! # Moving into tzcraft (from `rustix`)
//!
//! `rustix::time::Timespec` is a POSIX `{tv_sec, tv_nsec}` pair and ports in
//! one line: [`Ticks::from_timespec(tv_sec, tv_nsec)`](crate::Ticks::from_timespec).
//! There is no other time surface in `rustix` to port.
//!
//! # Porting values exactly
//!
//! Every `tzcraft` accessor yields the same integers the reference
//! libraries use, so porting a *value* is exact — no information loss, no
//! rounding:
//!
//! | `tzcraft` accessor | yields |
//! | --- | --- |
//! | [`Ticks::to_unix_seconds`](crate::Ticks::to_unix_seconds) | `(i64 seconds, u32 nanos)` |
//! | [`Ticks::as_unix_nanos`](crate::Ticks::as_unix_nanos) | `i128` nanoseconds since the epoch |
//! | [`Ticks::to_timespec`](crate::Ticks::to_timespec) | `(i64, i64)` — the POSIX `timespec` pair |
//! | [`Date::parts`](crate::Date::parts) | `(i32 year, u32 month, u32 day)` |
//! | [`TimeOfDay::parts`](crate::TimeOfDay::parts) | `(u32 hour, u32 minute, u32 second, u32 nanos)` |
//! | [`Offset::as_seconds`](crate::Offset::as_seconds) | `i32` whole seconds |
//! | [`Duration::as_nanos`](crate::Duration::as_nanos) / `num_*` | spans in nanoseconds / coarser units |
//!
//! # Directionality, stated plainly
//!
//! Moving **into** `tzcraft` is the supported, documented path above.
//! Moving **out** is left to the user: `tzcraft` ships no conversion impls
//! to `chrono` / `time` / `rustix` because those crates are not in the
//! dependency graph. If you must leave, the tables above read in reverse —
//! the integers are the same either way.
//!
//! # Verification
//!
//! [`tests/chrono_parity.rs`](https://github.com/blueokanna/Tzcraft/blob/main/tests/chrono_parity.rs)
//! exercises the migration surface using real `chrono` idioms
//! (constructors, strftime, checked arithmetic, codecs) against the
//! `tzcraft` API, so the porting path stays honest as the crate evolves.
