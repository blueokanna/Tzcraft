# Tzcraft

[![CI](https://github.com/blueokanna/Tzcraft/actions/workflows/ci.yml/badge.svg)](https://github.com/blueokanna/Tzcraft/actions/workflows/ci.yml)
[![docs.rs](https://img.shields.io/docsrs/tzcraft)](https://docs.rs/tzcraft)

A date and time library for Rust built on one idea: **one timeline, and
everything else is a projection**. The whole axis is a signed 128-bit
nanosecond counter (`Ticks`); the proleptic Gregorian calendar, weekdays,
ISO weeks and timezone offsets are pure projections onto that axis. No web
of `Add` impls, no global "current timezone" variable, no IANA database
downloads at runtime, no `unsafe`.

The crate is `#![no_std]` in every configuration and, with the default
features off, builds **without an allocator at all**.

## Why another time library

The existing reference libraries carry shapes that this project deliberately
does not want:

- `chrono` bakes the timezone into the type parameter; `DateTime<Tz>` drags
  generics everywhere and every operation is implemented several times.
- `time` stores an offset plus `i64` nanoseconds since midnight, squeezing
  the range and making overflow a thing to think about.
- `jiff` is pleasant, but behind it sit the IANA database, runtime state and
  a heavy dependency tree.

`tzcraft` swaps in a different set of assumptions. Each one is a fact about
the code you can check yourself.

**1. One timeline, one source of arithmetic.**
`Ticks` is the only instant type: a signed 128-bit nanosecond count since
the Unix epoch (`1970-01-01T00:00:00Z`). 128 bits buys full nanosecond
precision *and* a range of roughly ±292 billion years — no "small instant /
big instant" split, no overflow-collapse strategy to memorize. `Duration` is
a separate *signed* span type: the type system refuses to let you add two
instants, because `Ticks + Ticks` does not compile.

**2. Civil types are projections, not owners.**
`Date`, `TimeOfDay` and `CivilDateTime` carry no arithmetic of their own.
Midnight-crossing additions and year-boundary carries project onto the
single `i128` nanosecond axis and are computed once. Calendar-aware
operations (months, years) exist in exactly one place — project, adjust,
re-project — so you never hunt for the right impl among a dozen type
combinations.

**3. The compiler is the calendar.**
Leap rules, the day-count ↔ civil inversion, weekdays, ISO weeks — all
`const fn`. The compiler folds calendar math at compile time:

```rust
use tzcraft::{Date, Weekday};

const NEW_YEAR_2025: Date = Date::from_days_since_epoch(20_089);
const WD: Weekday = NEW_YEAR_2025.weekday(); // the compiler says: Wednesday
assert_eq!(WD, Weekday::Wednesday);
```

Timezones are `const` data too: a `Zone` is either `Utc` or a fixed
`Offset`, carried inline with the instant in `Zoned`. No global registry, no
mutable "current zone", no hidden context. `Zoned` is therefore `Copy` +
`Send` + `Sync` for free.

**4. The codec picks the wire shape.**
Every type implements `nextjson`'s format-neutral contract
(`NsonSerialize` / `NsonDeserialize`) exactly once. At encode time it asks
`is_human_readable()`:

| Type | Human-readable (nextjson JSON) | Binary (rustbinary) |
| --- | --- | --- |
| `Ticks` | RFC 3339 string | `i128` nanoseconds |
| `Duration` | ISO 8601 duration string | `i128` nanoseconds |
| `Date` | `YYYY-MM-DD` | `i32` days |
| `TimeOfDay` | `HH:MM:SS[.f]` | `u64` ns of day |
| `CivilDateTime` | zone-less ISO string | packed `i128` |
| `Offset` | `+08:00` / `Z` | `i32` seconds |
| `Zone` | `UTC` / offset string | tagged array |
| `Zoned` | RFC 3339 with offset | `[ticks, offset]` array |
| `Weekday` | `"Monday"` (numbers also accepted) | `u8` discriminant |
| `Month` | `"January"` (numbers also accepted) | `u8` month number |

JSON stays readable and self-describing, the binary profile stays compact —
no separate serde module, no feature that silently changes the format. One
implementation, two shapes.

## `no_std`, with or without an allocator

The crate is `#![no_std]` in every build. The `alloc` feature (on by
default, because `std` implies it) only gates the APIs that return an owned
`String` and the codecs:

| Feature | What it enables |
| --- | --- |
| `std` (default) | `Ticks::now()`, `to_std_time`, `std::error::Error` |
| `alloc` (default via `std`) | `to_rfc3339`, `format`, `to_iso`, `to_iso8601`, ... (the `String`-returning methods) |
| `serde` (default) | `nextjson` text/binary codec implementations (`tzcraft::codec`) |
| `binary` (default) | `rustbinary` compact wire (`tzcraft::binary`) |

With `--no-default-features` the crate links **no allocator**: parsing,
arithmetic, `Display`/`FromStr`, and every `write_*` buffer-formatting
method keep working. Every `String`-returning method has an allocator-free
twin that writes into a caller-owned slice and returns the byte count:

```rust
use tzcraft::{Date, Ticks};

let mut buf = [0u8; 64];
let d = Date::from_ymd(2024, 2, 29).unwrap();
let n = d.write_iso(&mut buf).unwrap();
assert_eq!(&buf[..n], b"2024-02-29");

let n = Ticks::EPOCH
    .write_rfc3339(&mut buf, tzcraft::FractionDigits::None)
    .unwrap();
assert_eq!(&buf[..n], b"1970-01-01T00:00:00Z");
```

The allocator-free sinks are exposed as `tzcraft::write::{Write, Buf}`,
ready for embedded targets.

## Quick start

```rust
use tzcraft::{Date, Duration, Months, Offset, Ticks, Weekday, Zone, Zoned};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // One timeline, any reading.
    let launch = Ticks::from_rfc3339("2024-06-15T08:30:00Z")?;
    let local = launch.to_zoned(Zone::fixed(Offset::from_hms(8, 0, 0)?));
    assert_eq!(local.to_rfc3339(tzcraft::FractionDigits::None), "2024-06-15T16:30:00+08:00");
    assert_eq!(local.date()?.weekday(), Weekday::Saturday);

    // Calendar-aware months clamp instead of overflowing.
    let jan = Date::from_ymd(2023, 1, 31)?;
    assert_eq!(jan.checked_add_months(Months::new(1))?, Date::from_ymd(2023, 2, 28)?);
    assert_eq!(jan.checked_add_months(Months::new(13))?, Date::from_ymd(2024, 2, 29)?); // leap year

    // Durations are signed and round-trip ISO 8601 exactly.
    let span = Duration::from_iso8601("P1DT2H3M4.5S")?;
    assert_eq!(span.to_iso8601(), "P1DT2H3M4.5S");

    // Text and binary share one implementation.
    let json = nextjson::nextencode(&local)?;
    let back: Zoned = nextjson::nextdecode(&json)?;
    assert_eq!(back, local);

    let bin = tzcraft::binary::encode(&local)?;
    let back: Zoned = tzcraft::binary::decode(&bin)?;
    assert_eq!(back, local);
    Ok(())
}
```

A fixed timezone is one line of `const`:

```rust
use tzcraft::{Offset, Zone};

const TOKYO: Zone = Zone::fixed(Offset::east(9 * 3600));
```

Mixing into a derived struct is natural (the derive macros come from
`nextjson`):

```rust
#[derive(Debug, PartialEq, nextjson::NsonSerialize, nextjson::NsonDeserialize)]
struct Alarm {
    name: String,
    when: Zoned,
    repeat: tzcraft::Weekday,
    snooze: tzcraft::Duration,
}
```

## Y2038: safe by construction

The Year 2038 problem is a 32-bit signed-seconds overflow
(`i32::MAX` seconds after the epoch = `2038-01-19T03:14:07Z`). `tzcraft`
cannot hit it:

- `Ticks` is `i128` nanoseconds since the epoch — range ≈ ±292 billion
  years;
- every `timestamp*` accessor and `from_timestamp*` constructor uses `i64`
  seconds — range ≈ ±292 billion years;
- `Date` is `i32` **days** since the epoch (≈ ±5.8 million years), never
  seconds;
- the only `i32` time-domain value, `Offset`, is bounded to ±24 h and
  validated at construction.

`tests/y2038.rs` pins the boundary behaviour: the exact rollover second
(`2_147_483_647` → `2_147_483_648`), pre-epoch extremes, expanded years past
9999, and text round-trips across the boundary.

## Moving from chrono

`tzcraft` covers the everyday `chrono` API. `_opt` variants become plain
`?` in our `Result` model:

| chrono | tzcraft |
| --- | --- |
| `Utc::now()` / `Local::now()` | `Ticks::now()?` / `Zoned::now_utc()?` |
| `DateTime::<Utc>` | `Ticks` |
| `DateTime::<FixedOffset>` | `Zoned` |
| `NaiveDate` | `Date` |
| `NaiveTime` | `TimeOfDay` |
| `NaiveDateTime` | `CivilDateTime` |
| `Duration` / `TimeDelta` | `Duration` (same-named `seconds/hours/days/weeks/num_*`) |
| `from_ymd_opt` / `from_hms_opt` | `Date::from_ymd` / `TimeOfDay::from_hms` (return `Result`) |
| `DateTime::from_timestamp` / `timestamp()` | `Ticks::from_timestamp` / `timestamp()` (plus `_millis`/`_micros`/`_nanos`) |
| `date.and_hms_opt(...)` | `date.and_hms(...)?` |
| `d.checked_add_months(Months::new(1))` | same name, same signature |
| `d.checked_add_days(Days::new(1))` | same name, same signature |
| `dt.format("%Y-%m-%d %H:%M:%S")` | `dt.format("%Y-%m-%d %H:%M:%S")?` (unknown directives are errors, not silently dropped) |
| `NaiveDate::parse_from_str` / `NaiveDateTime::parse_from_str` | `Date::parse_from_str(s, fmt)` / `CivilDateTime::parse_from_str(s, fmt)` |
| `DateTime::parse_from_str` | `Ticks::parse_from_str` / `Zoned::parse_from_str` (the civil path needs a timezone offset) |
| `to_rfc3339` / `parse_from_rfc3339` | `to_rfc3339(frac)` / `from_rfc3339` |
| `to_rfc2822` / `parse_from_rfc2822` | same names |
| `Datelike::year/month/day/ordinal/weekday/iso_week/num_days_from_ce` | same-named inherent methods |
| `Timelike::hour/minute/second/nanosecond/num_seconds_from_midnight` | same-named inherent methods |
| `FixedOffset::east_opt/from_hms_opt` | `Offset::from_seconds` / `Offset::from_hms` |
| `dt.with_timezone(...)` | `z.with_zone(...)` |
| `checked_add_signed` / `signed_duration_since` | same names |
| `with_year/with_month/.../with_nanosecond` | same names |
| `Duration::to_std/from_std` | same names |
| serde support | nextjson `NsonSerialize` / `NsonDeserialize` (text + rustbinary binary) |

Three deliberate differences. They are correctness or safety calls, not
oversights:

1. **`format()` returns `Result`** — unknown directives are errors instead
   of being silently dropped.
2. **`timestamp()` floors** — a pre-epoch instant like
   `1969-12-31T23:59:59.5Z` maps to `-1`, which is what Unix time means;
   chrono truncates toward zero and would give `0`.
3. **`num_*` returns `Result<i64>`** — out-of-range values are explicit
   errors instead of silent overflow.

`Local` (the system-local zone) is not in v1: pure `std` cannot read the
local offset (that needs `libc`/platform FFI, and the core crate only
depends on `nextjson` and `rustbinary` while denying `unsafe`). If your wall
clock must follow the real local zone, resolve the offset with a platform
API and hand it to `Zone::fixed(...)` — the seam is explicit.

## Migration: bring `chrono` / `time` / `rustix` code in

Migration is **one-way easy: into `tzcraft`**. The crate does not depend on
`chrono`, `time` or `rustix` — its only dependencies are `nextjson` and
`rustbinary` (both optional) — so there is nothing that links those crates
and nothing that makes leaving easier than arriving. The tables above (and
the `time`/`rustix` equivalents in the
[`tzcraft::migration`](https://docs.rs/tzcraft/latest/tzcraft/migration/index.html)
module documentation) show the mechanical, name-identical port.

- `chrono`: see the table above — `DateTime`/`NaiveDate`/`NaiveTime`/
  `NaiveDateTime`/`Duration`/`FixedOffset` map to `Ticks`/`Date`/
  `TimeOfDay`/`CivilDateTime`/`Duration`/`Offset`.
- `time`: `OffsetDateTime` → `Zoned`, `PrimitiveDateTime` → `CivilDateTime`,
  `Date` → `Date`, `Time` → `TimeOfDay`, `UtcOffset` → `Offset`, `Duration`
  → `Duration`.
- `rustix`: `Timespec` (a POSIX `{tv_sec, tv_nsec}` pair) →
  `Ticks::from_timespec(tv_sec, tv_nsec)`.

Porting a *value* is exact: every `tzcraft` accessor yields the same
integers the reference libraries use (`Ticks::to_unix_seconds` →
`(i64, u32)`, `Date::parts`, `TimeOfDay::parts`, `Offset::as_seconds`,
`Duration::as_nanos`). The full mapping and worked examples are in the
`tzcraft::migration` module.

## What's deliberately not here

- **No IANA database, no DST.** `Zone` is `Utc` or a fixed offset. If a wall
  clock must follow real transitions, resolve the offset yourself and hand
  the resulting `Zone::Fixed` to the library. The seam is intentionally
  narrow, and leaves room for a future `Zone::Database` variant.
- **Only the proleptic Gregorian calendar** (year 0 = 1 BCE). No Julian, no
  Hebrew, no other calendars.
- **Strict ISO 8601 / RFC 3339 / RFC 2822 plus strftime.** What we ship is
  complete and tested; there is no half-finished template engine.
- **No `unsafe`.** Dependencies: `nextjson` (codecs) and `rustbinary`
  (binary), both optional. No third-party date/time library appears anywhere
  in the dependency graph.

## Layout

```text
src/
  calendar.rs   civil calendar core: leap rules, day<->civil, weekday, ISO week — all const fn
  units.rs      Days / Months / IsoWeek (chrono-compatible typed units)
  ticks.rs      Ticks: the only instant type, i128 nanoseconds
  duration.rs   Duration: signed spans
  date.rs       Date: i32-day projection
  time.rs       TimeOfDay: u64 ns of day
  datetime.rs   CivilDateTime: zone-less date-time
  offset.rs     Offset: seconds within +/-24h
  zone.rs       Zone: Utc / fixed offset
  zoned.rs      Zoned: Ticks + Zone
  write.rs      allocator-free Write trait + Buf sinks (no_std, no alloc)
  format.rs     hand-written ISO / RFC 3339 parser & formatter
  strftime.rs   strftime engine + RFC 2822 (the chrono-compatible surface)
  codec.rs      nextjson contract impls (human-readable vs binary branch)
  binary.rs     rustbinary facade
  migration.rs  guide: bringing chrono / time / rustix code in
```

The parsers scan bytes one at a time and every failure carries a byte
offset. Fractional seconds are capped at 9 digits — anything more is
rejected, never truncated. Truncation would be lying about the data.

## Tests & safety

- Calendar: day-by-day round trips over ±200,000 days, the full calendar
  round trip over 6,000 years, weekday anchors, ISO week boundary vectors.
- Formatting: RFC 3339 / RFC 2822 / strftime round trips plus a reject-list
  of malformed inputs.
- Codec: every type round-trips through both nextjson text and rustbinary
  binary, plus derived structs that mix tzcraft types.
- `chrono` parity: `tests/chrono_parity.rs` exercises the migration surface
  using real chrono idioms.
- Y2038: `tests/y2038.rs` pins the boundary, pre-epoch extremes and expanded
  years.
- No-alloc: `tests/no_alloc.rs` runs only with `--no-default-features` and
  proves parsing, `Display`/`FromStr` and the `write_*` buffer APIs work
  without an allocator.
- Robustness: `tests/robustness.rs` feeds thousands of adversarial inputs
  (random bytes, oversized inputs, malformed structures, extreme numbers,
  hostile format strings) through every parse and format entry point. The
  contract: no input may panic or allocate without bound.
- Low-level audit: every `as` narrowing cast and `i64`-wide multiply was
  reviewed by hand. The audit found and fixed overflow in
  `Duration::from_days/minutes/hours/weeks` (an `i64` multiply that panicked
  in debug and wrapped in release at `i64::MAX`), a narrowing wrap in
  `Ticks`/`Zoned::checked_add_days(Days)` for `u64` day counts past `i64`,
  unchecked `i128` additions in `duration_since` and `checked_add`, and an
  `as i64` wrap when formatting `%s` for extreme instants. Regression tests
  lock each one down.
- Dependencies: `cargo audit` reports zero known vulnerabilities.

```sh
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo audit
```

## Benchmarking

[`benchmark.md`](./benchmark.md) holds the comparison report: tzcraft vs
`chrono` 0.4.45 / `time` 0.3.55 / `jiff` 0.2.35, for RFC 3339 parse and
format, civil projection, date arithmetic, duration arithmetic and weekday,
plus a panic-freedom fuzz run and a dependency/`unsafe` footprint.

The harness lives in the **`benchmarks/`** package (`publish = false`). It
is the only place the three comparison libraries appear; they are never part
of tzcraft's dependency graph. A GitHub Action re-runs it on every push to
`main` and commits the fresh numbers back to `benchmark.md`.

Methodology is documented in [`benchmarks/README.md`](./benchmarks/README.md)
and inline in the report: identical inputs, `black_box`, varied input arrays
to defeat loop-invariant code motion, and a minimum-of-three timing scheme.
Numbers are machine-specific and only comparable within a single run.

## CI

`.github/workflows/ci.yml` runs on every push and pull request: formatting,
clippy with `-D warnings`, debug and release tests, the feature matrix
(`--no-default-features` and each of `alloc` / `std` / `serde` / `binary`),
docs built with `-D warnings`, a `docs.rs`-condition build (nightly with
`--cfg docsrs`, the exact flags docs.rs uses), a security audit against the
RustSec advisory database, and the full test suite at the declared MSRV
**1.81**.

## License

Apache-2.0. 中文版 README：[README_CN.md](./README_CN.md)。
