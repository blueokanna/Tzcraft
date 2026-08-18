# no_std and Features

## The short version

The crate is `#![no_std]` in **every** configuration. With the default
features it links `alloc`; with `--no-default-features` it links no
allocator at all.

## Feature matrix

| Feature | Default | What it enables |
| --- | --- | --- |
| `std` | on | `Ticks::now()`, `Ticks::to_std_time`, `impl std::error::Error` |
| `alloc` | on (implied by `std`) | the `String`-returning formatting methods (`to_rfc3339`, `format`, `to_iso`, `to_iso8601`, ...) |
| `serde` | on | `nextjson` codec implementations (`tzcraft::codec`) |
| `binary` | on | `rustbinary` compact wire format (`tzcraft::binary`) |

There are no other features, and no third-party date/time library in the
dependency graph. The only dependencies are `nextjson` and `rustbinary`, both
optional.

Feature implications in `Cargo.toml`:

```toml
std = ["alloc"]
alloc = []
serde = ["dep:nextjson", "alloc"]
binary = ["dep:rustbinary", "serde", "alloc"]
```

`std` implies `alloc`; the codecs need `alloc` too (they produce and consume
`String`s). Turning all defaults off gives a build with no allocator
dependency.

## What works without an allocator

With `--no-default-features`:

- parsing (`from_rfc3339`, `from_iso`, `from_iso8601`, `parse_from_str`,
  `FromStr`) — the parsers are byte scanners over `&[u8]` with no heap use;
- arithmetic (checked/saturating add-sub, month/year stepping, durations);
- `Display` on every type — writes directly into the `fmt::Formatter`;
- `FromStr` on every type;
- every `write_*` buffer method — writes into a caller-owned `&mut [u8]` and
  returns the byte count:
  - `Ticks::write_rfc3339` / `write_rfc2822` / `write_format`
  - `Date::write_iso` / `write_format`
  - `TimeOfDay::write_iso` / `write_format`
  - `CivilDateTime::write_iso` / `write_format`
  - `Duration::write_iso8601`
  - `Offset::write_iso`
  - `Zone::write_iso`
  - `Zoned::write_rfc3339` / `write_rfc2822` / `write_format`

## What needs the `alloc` feature

Every method that returns an owned `String`: `to_rfc3339`, `to_rfc2822`,
`format`, `to_iso`, `to_iso8601`, `to_iso` (on `Offset`/`Zone`), and the
`codec`/`binary` modules. Each of these is a thin wrapper over the same
`write_*` machinery — the strings are built into a fresh `String` that
implements the crate's `Write` trait.

On docs.rs (built with `--all-features`) these are marked with the "Available
on crate feature alloc only" badge.

## The `write` module

```rust
use tzcraft::write::{Buf, Write};

let mut storage = [0u8; 64];
let mut buf = Buf::new(&mut storage);
buf.write_str("hello")?;
assert_eq!(buf.as_str(), "hello");
```

`tzcraft::write::Write` is a three-method trait (`write_bytes` plus defaulted
`write_str`/`write_byte`/`write_char`). The crate does not depend on
`core::fmt::Write` (which is not available on every `no_std` target), so it
defines its own minimal trait. `Buf` is the fixed-capacity sink; a buffer
that is too small yields `Error::buffer_overflow()` — never a panic or a
silent truncation.

## Cargo.toml snippet

```toml
[dependencies]
tzcraft = { version = "0.1", default-features = false }   # no allocator
tzcraft = { version = "0.1", default-features = false, features = ["alloc"] }
tzcraft = { version = "0.1" }                              # defaults: std + serde + binary
```

## MSRV

The declared minimum Rust version is **1.81**. All feature combinations are
tested at that version in CI.
