# Migration Guide

This page documents moving **into** tzcraft from `chrono`, `time` or
`rustix`. Migration is deliberately **one-way easy: into tzcraft**. The
crate's dependency graph contains no third-party date/time library — only
`nextjson` and `rustbinary` (both optional) — so there is nothing here that
links those crates and nothing that makes leaving easier than arriving.

## Moving into tzcraft (from chrono)

The everyday `chrono` surface maps 1:1. `_opt` variants become plain `?` on
our `Result` model.

| chrono | tzcraft |
| --- | --- |
| `Utc::now()` / `Local::now()` | `Ticks::now()?` / `Zoned::now_utc()?` |
| `DateTime::<Utc>` | `Ticks` |
| `DateTime::<FixedOffset>` | `Zoned` |
| `NaiveDate` | `Date` |
| `NaiveTime` | `TimeOfDay` |
| `NaiveDateTime` | `CivilDateTime` |
| `Duration` / `TimeDelta` | `Duration` |
| `from_ymd_opt` / `from_hms_opt` | `Date::from_ymd` / `TimeOfDay::from_hms` |
| `DateTime::from_timestamp` / `timestamp()` | `Ticks::from_timestamp` / `timestamp()` |
| `date.and_hms_opt(...)` | `date.and_hms(...)?` |
| `d.checked_add_months(Months::new(1))` | same name, same signature |
| `d.checked_add_days(Days::new(1))` | same name, same signature |
| `dt.format(...)` | `dt.format(...)?` |
| `NaiveDate::parse_from_str` | `Date::parse_from_str(s, fmt)` |
| `DateTime::parse_from_str` | `Ticks::parse_from_str` / `Zoned::parse_from_str` |
| `to_rfc3339` / `parse_from_rfc3339` | `to_rfc3339(frac)` / `from_rfc3339` |
| `to_rfc2822` / `parse_from_rfc2822` | same names |
| `Datelike::*` / `Timelike::*` | same-named inherent methods |
| `FixedOffset::east_opt` | `Offset::from_seconds` |
| `dt.with_timezone(...)` | `z.with_zone(...)` |
| `checked_add_signed` / `signed_duration_since` | same names |
| `with_year/with_month/...` | same names |
| `Duration::to_std/from_std` | same names |

Three deliberate differences (correctness calls, not oversights):

1. **`format()` returns `Result`** — unknown directives are errors instead
   of being silently dropped.
2. **`timestamp()` floors** — `1969-12-31T23:59:59.5Z` maps to `-1`, which
   is what Unix time means; chrono truncates toward zero and gives `0`.
3. **`num_*` returns `Result<i64>`** — out-of-range values are explicit
   errors instead of silent overflow.

`Local` (the system-local zone) is out of scope without platform FFI;
resolve the offset yourself and hand it to `Zone::fixed(...)`.

## Moving into tzcraft (from time)

| time | tzcraft |
| --- | --- |
| `OffsetDateTime` | `Zoned` (or `Ticks` for UTC) |
| `PrimitiveDateTime` | `CivilDateTime` |
| `Date` | `Date` |
| `Time` | `TimeOfDay` |
| `UtcOffset` | `Offset` |
| `Duration` | `Duration` |
| `Weekday` / `Month` | `Weekday` / `Month` |

`OffsetDateTime::unix_timestamp()` + `.nanosecond()` feed
`Ticks::from_timestamp`; `Date::year/month/day` and
`Time::hour/minute/second/nanosecond` feed `Date::from_ymd` and
`TimeOfDay::from_hms_nano`.

## Moving into tzcraft (from rustix)

`rustix::time::Timespec` is a POSIX `{tv_sec, tv_nsec}` pair and ports in
one line: `Ticks::from_timespec(tv_sec, tv_nsec)`. There is no other time
surface in `rustix` to port.

## Porting values exactly

Every `tzcraft` accessor yields the same integers the reference libraries
use, so porting a value is exact — no information loss, no rounding:

| tzcraft accessor | yields |
| --- | --- |
| `Ticks::to_unix_seconds()` | `(i64 seconds, u32 nanos)` |
| `Ticks::as_unix_nanos()` | `i128` nanoseconds since the epoch |
| `Ticks::to_timespec()` | `(i64, i64)` — the POSIX `timespec` pair |
| `Date::parts()` | `(i32 year, u32 month, u32 day)` |
| `TimeOfDay::parts()` | `(u32 hour, u32 minute, u32 second, u32 nanos)` |
| `Offset::as_seconds()` | `i32` whole seconds |
| `Duration::as_nanos()` / `num_*` | spans in nanoseconds / coarser units |

## Directionality, stated plainly

Moving **into** tzcraft is the supported, documented path above. Moving
**out** is left to the user: tzcraft ships no conversion impls to `chrono` /
`time` / `rustix` because those crates are not in the dependency graph. If
you must leave, the tables above read in reverse — the integers are the same
either way.

## Verification

`tests/chrono_parity.rs` exercises the migration surface using real chrono
idioms (constructors, strftime, checked arithmetic, codecs) against the
tzcraft API, so the porting path stays honest as the crate evolves.
