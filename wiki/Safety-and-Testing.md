# Safety and Testing

## The guarantees

- **No `unsafe`.** `#![deny(unsafe_code)]` is on crate-wide; there is no
  `unsafe` block anywhere in the source.
- **No input can panic.** The parsers accept untrusted bytes; the contract
  is that no input of any length or content may cause a panic, an unbounded
  allocation, or non-deterministic behaviour. Malformed input is rejected
  with an error (carrying the byte offset of the failure).
- **No silent truncation.** Fractional seconds are capped at 9 digits;
  anything more is rejected, never truncated.
- **No silent overflow.** Every `checked_*` operation returns an explicit
  error. Unit-scaled duration constructors compute in `i128`. `num_*`
  returns `Result<i64>` instead of wrapping.
- **Y2038-safe by construction.** No code path stores seconds in 32 bits.
  See [Y2038](Y2038).

## Audit methodology

The low-level audit is a manual review of every

- `as` narrowing cast,
- `i64`- and `i128`-wide multiply,
- `unwrap` / `expect` / `panic!` / `unreachable!` in non-test code, and
- unchecked add/sub in the timeline arithmetic.

Previous audit rounds found and fixed, with regression tests:

1. `Duration::from_days/minutes/hours/weeks` overflowed in the `i64` domain
   (panicked in debug, wrapped silently in release at `i64::MAX`). Fixed by
   computing in `i128`.
2. `Ticks`/`Zoned::checked_add_days(Days)` narrowed a `u64` day count past
   `i64` and wrapped negative. Fixed with explicit `i64::try_from`.
3. `Ticks::duration_since` and `CivilDateTime::checked_add` had unchecked
   `i128` additions (`MAX - MIN` overflow). Fixed with `saturating_sub` and
   `checked_add` chains.
4. `%s` formatting narrowed nanoseconds to `i64` seconds and wrapped for
   extreme instants. Fixed to render nothing for out-of-range values.
5. The extreme-instant fallback of `Ticks`/`Zoned::write_rfc3339` and
   `write_rfc2822` cast negative `i128` instants with `as u128`, wrapping
   them into a huge positive on the wire. Fixed with a sign-aware emitter
   so the allocating and buffer paths agree.

The remaining `unwrap`/`expect`/`panic!` sites in non-test code are
invariant-based (a validated `TimeOfDay` is always a valid `chrono::NaiveTime`
/ `time::Time`; a fresh `String` never overflows; `Buf` only ever holds
valid UTF-8) or documented const-constructor panics (`Offset::east`/`west`,
mirroring `chrono::FixedOffset::east`).

## Test suites

| Suite | What it verifies |
| --- | --- |
| `src/**` unit tests | Calendar day/civil round trips over ±200,000 days, the full calendar over 6,000 years, weekday anchors, ISO week boundaries, formatting vectors, parse reject-lists |
| `tests/chrono_parity.rs` | The migration surface using real `chrono` idioms |
| `tests/y2038.rs` | The 2038 boundary, pre-epoch extremes, expanded years |
| `tests/no_alloc.rs` | The allocator-free surface (`--no-default-features` only) |
| `tests/robustness.rs` | Thousands of adversarial inputs (random bytes, oversized inputs, malformed structures, extreme numbers, hostile format strings) through every parse and format entry point |
| `tests/readme.rs` | The README snippets compile and run |
| doctests | The crate-level and module-level examples |

Run everything:

```sh
cargo test --all-features          # debug
cargo test --all-features --release  # release (no overflow checks; both must agree)
cargo test --no-default-features  # allocator-free build
cargo clippy --all-features --all-targets -- -D warnings
```

## Dependencies

`cargo audit` (RustSec advisory database) reports zero known vulnerabilities
against the locked dependency graph (which contains only `nextjson` and
`rustbinary` in addition to `tzcraft` itself).

## CI

`.github/workflows/ci.yml` runs on every push and pull request:

- `cargo fmt --check`
- `cargo clippy --all-features --all-targets -- -D warnings`
- `cargo test --all-features` (debug and release)
- feature matrix: `--no-default-features` and each of `alloc` / `std` /
  `serde` / `binary`
- `cargo doc --all-features --no-deps` with `-D warnings`
- docs.rs-condition build: nightly + `--cfg docsrs` (the exact flags
  docs.rs uses)
- `cargo audit` via `rustsec/audit-check`
- the full test suite at MSRV **1.81**
