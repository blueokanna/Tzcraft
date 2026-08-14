# Tzcraft

[![CI](https://github.com/blueokanna/Tzcraft/actions/workflows/ci.yml/badge.svg)](https://github.com/blueokanna/Tzcraft/actions/workflows/ci.yml)

A date and time library for Rust that doesn't copy the usual playbook.

The core idea is plain: **one timeline, and everything else is a
projection**. The whole axis is a signed 128-bit nanosecond counter
(`Ticks`); the Gregorian calendar, weekdays, ISO weeks and timezone offsets
are all pure projections onto that axis. No web of `Add` impls, no global
"current timezone" variable, no IANA database downloads at runtime, no
`unsafe`.

## Why another time library

Because the existing ones carry shapes I don't want.

- `chrono` bakes the timezone into the type parameter; `DateTime<Tz>` drags
  generics everywhere and every operation gets implemented several times.
- `time` uses `i64` nanoseconds plus an offset, which squeezes the range to
  about ±167 years and makes you think about overflow.
- `jiff` is nice, but behind it sit the IANA database, runtime state, and a
  heavy dependency tree.

`tzcraft` swaps in a different set of assumptions. Each one is a fact about
the code you can check yourself.

**1. One timeline, one source of arithmetic.**
`Ticks` is the only instant type: a signed 128-bit nanosecond count since
the Unix epoch (`1970-01-01T00:00:00Z`). 128 bits buys full nanosecond
precision *and* a range of roughly ±292 billion years — no "small instant /
big instant" split, no overflow-collapse strategy to memorize. `Duration` is
a separate *signed* span type: the type system refuses to let you add two
instants, because `Ticks + Ticks` simply doesn't compile.

**2. Civil types are projections, not owners.**
`Date`, `TimeOfDay` and `CivilDateTime` carry no arithmetic of their own.
Midnight-crossing additions and year-boundary carries all project onto the
single `i128` nanosecond axis and get computed once. Calendar-aware
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

`Local` (the system-local zone) is not in v1: pure `std` can't read the
local offset (that needs `libc`/platform FFI, and this crate only depends on
`nextjson` and `rustbinary` while denying `unsafe`). If your wall clock must
follow the real local zone, resolve the offset with a platform API and hand
it to `Zone::fixed(...)` — the seam is explicit.

## What's deliberately not here

- **No IANA database, no DST.** `Zone` is `Utc` or a fixed offset. If a wall
  clock must follow real transitions, resolve the offset yourself and hand
  the resulting `Zone::Fixed` to the library. The seam is intentionally
  narrow, and leaves room for a future `Zone::Database` variant.
- **Only the proleptic Gregorian calendar** (year 0 = 1 BCE). No Julian, no
  Hebrew, no other calendars.
- **Strict ISO 8601 / RFC 3339 / RFC 2822 plus strftime.** What we ship is
  complete and tested; there's no half-finished template engine.
- **No `unsafe`, no dependencies beyond `nextjson` and `rustbinary`.**
  `no_std + alloc`; default features are `std`, `serde`, `binary`, each can
  be switched off.

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
  format.rs     hand-written ISO / RFC 3339 parser & formatter
  strftime.rs   strftime engine + RFC 2822 (the chrono-compatible surface)
  codec.rs      nextjson contract impls (human-readable vs binary branch)
  binary.rs     rustbinary facade
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
- chrono parity: `tests/chrono_parity.rs` exercises the migration surface
  using real chrono idioms.
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
cargo clippy --all-features --all-targets
cargo audit
```

## CI

`.github/workflows/ci.yml` runs on every push and pull request: formatting,
clippy with `-D warnings`, debug and release tests, the feature matrix
(no-default / `std` / `serde` / `binary`), docs built with `-D warnings`, a
`docs.rs`-condition build (nightly with `--cfg docsrs`, the exact flags
docs.rs uses), a security audit against the RustSec advisory database, and
the full test suite at the declared MSRV **1.81** (the floor is set by
`rustbinary`, which needs `error_in_core`).

## License

Apache-2.0. 中文版 README：[README_CN.md](./README_CN.md)。
