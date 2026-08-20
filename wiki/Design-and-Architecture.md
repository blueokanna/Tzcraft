# Design and Architecture

## The four axioms

tzcraft is built on four decisions. Each is a fact about the code, not a
slogan.

### 1. One timeline, one source of arithmetic

`Ticks` is the only instant type: a signed 128-bit nanosecond counter since
`1970-01-01T00:00:00Z` (proleptic Gregorian). The width is the design:

- **Precision.** Nanosecond resolution for every representable instant.
- **Range.** `i128` nanoseconds spans roughly ±5.4×10^21 years (the
  `i64`-second accessors bound the usable range to ≈ ±292 billion years).
  There is no "small instant / large instant" split, no overflow-collapse
  strategy, no second type to remember.

`Duration` is a distinct signed span type (`i128` nanoseconds too, so the
conversion is free). Because the types are distinct, `Ticks + Ticks` does
not compile — the type system refuses to add two instants.

### 2. Civil types are projections, not owners

`Date` (i32 days since the epoch), `TimeOfDay` (u64 nanoseconds of the day)
and `CivilDateTime` (a `Date` + a `TimeOfDay`) hold **no arithmetic of
their own**. Midnight-crossing additions and year-boundary carries project
onto the single `i128` nanosecond axis and are computed once in the
timeline projection code (`calendar::ns_divmod_day` and friends).

Calendar-aware operations (months, years) exist in exactly one place:
project → adjust → re-project. This removes the entire matrix of `Add`
impls that `chrono` has to maintain.

### 3. The compiler is the calendar

Leap rules, the day-count ↔ civil inversion (Hinnant's algorithm), weekdays
and ISO weeks are all `const fn`. A `const` date, a static zone table, or
an array of ISO week numbers is folded by the compiler at compile time.

Timezones are `const` data too. A `Zone` is one of:

- `Zone::Utc`, or
- `Zone::Fixed(Offset)` where `Offset` is a signed whole-second displacement
  within the open interval `(-24h, 24h)`.

A `Zoned` is `Ticks + Zone`, carried inline as a value. There is no global
registry, no mutable "current zone", no hidden context — `Zoned` is
therefore `Copy` + `Send` + `Sync`.

### 4. The codec picks the wire shape

Every type implements `nextjson`'s format-neutral contract
(`NsonSchema` + `NsonSerialize` + `NsonDeserialize`) exactly once. At encode
time it asks `is_human_readable()`:

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

One implementation, two wire shapes, zero feature toggles.

## Type model

```text
                      ┌────────────────────────────┐
                      │  Ticks (i128 ns since epoch)│  ← the only owner of arithmetic
                      └─────────────┬──────────────┘
                                    │ project
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
        CivilDateTime (Date + TimeOfDay)      Zoned (Ticks + Zone)
        ┌──────────────┴─────────────┐            │
        ▼                            ▼            ▼
     Date (i32 days)          TimeOfDay (u64 ns)  Zone (Utc | Fixed(Offset))
```

The dependency flow keeps the pure math at the leaves:

- `calendar.rs` — pure `const fn` calendar math, no dependencies.
- `write.rs` — allocator-free `Write` trait and `Buf` sink, depends only on
  the error type.
- `units.rs`, `date.rs`, `time.rs` — civil projections over `calendar`.
- `ticks.rs`, `datetime.rs`, `zoned.rs` — the timeline and its projections.
- `format.rs`, `strftime.rs` — parsing and formatting over the types.
- `codec.rs`, `binary.rs` — wire formats (feature-gated).
- `migration.rs` — documentation of the one-way path from `chrono` / `time`
  / `rustix` into `tzcraft` (no dependency on those crates).

The `Ticks ↔ CivilDateTime` / `Ticks ↔ Zoned` pairs are deliberately
bidirectional: that is axiom 2 in action, and the projection arithmetic
lives in one place (`calendar.rs`), not spread across the modules.

## Module layout

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

## Deliberate exclusions

- **No IANA timezone database, no DST.** `Zone` is `Utc` or a fixed offset.
  If a wall clock must follow real transitions, resolve the offset with your
  own policy and hand the resulting `Zone::Fixed` to the library. The seam
  is narrow on purpose, and leaves room for a future `Zone::Database`
  variant.
- **Only the proleptic Gregorian calendar** (year 0 = 1 BCE). No Julian, no
  Hebrew, no other calendars.
- **Strict ISO 8601 / RFC 3339 / RFC 2822 plus a full strftime engine.**
  What is shipped is complete and tested; there is no half-finished template
  engine.
- **No `unsafe`.** `#![deny(unsafe_code)]` is on crate-wide.
