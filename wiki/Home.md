# Welcome to the tzcraft wiki

This wiki documents **tzcraft**, a Rust date and time library built on one
idea: *one timeline, and everything else is a projection*.

## Pages

| Page | What it covers |
| --- | --- |
| [Design and Architecture](Design-and-Architecture) | The four axioms, the type model, and why each design decision exists |
| [no_std and Features](no_std-and-Features) | Feature matrix, the allocator-free build, `write_*` buffer APIs |
| [Y2038 and Range](Y2038) | Why the Year 2038 problem cannot occur, and the pinned boundary tests |
| [Migration Guide](Migration-Guide) | Moving from `chrono`, `time` and `rustix` — and back out again |
| [Safety and Testing](Safety-and-Testing) | Audit methodology, adversarial testing, and the guarantees |
| [Publishing](Publishing) | crates.io and docs.rs setup for maintainers |

## Benchmarking

[`benchmark.md`](https://github.com/blueokanna/Tzcraft/blob/main/benchmark.md) is
the CI-generated comparison report: tzcraft vs `chrono` / `time` / `jiff`
(performance, panic-freedom fuzzing, dependency and `unsafe` footprint). The
harness lives in the `benchmarks/` package (`publish = false`); the three
comparison libraries never enter tzcraft's dependency graph.

中文读者请看[首页](Home-CN)。

## The short version

- `Ticks` is the only instant type: a signed 128-bit nanosecond count since
  the Unix epoch. Storage range ≈ ±5.4×10^21 years (the `i64`-second
  accessors bound the usable range to ≈ ±292 billion years), full nanosecond
  precision.
- `Date`, `TimeOfDay` and `CivilDateTime` are pure projections of that
  timeline; they hold no arithmetic of their own.
- Every civil computation is a `const fn`; timezones are `const` data
  (`Zone::Utc` or a fixed `Offset`).
- The crate is `#![no_std]` in every configuration, and with
  `--no-default-features` builds without an allocator at all.
- Codecs (`nextjson` text, `rustbinary` binary) are one implementation with
  two wire shapes.
- `tzcraft::migration` documents the one-way path: `chrono` / `time` /
  `rustix` code ports *into* `tzcraft` easily; the crate links none of
  those libraries.

## Quick links

- Repository: <https://github.com/blueokanna/Tzcraft>
- Documentation: <https://docs.rs/tzcraft>
- crates.io: <https://crates.io/crates/tzcraft>
