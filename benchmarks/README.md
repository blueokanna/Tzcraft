# tzcraft-benchmarks

Development-only comparison harness: **tzcraft vs chrono vs time vs jiff**.

This package is **never published** and is **not part of `tzcraft`'s
dependency graph**. `tzcraft` itself depends only on `nextjson` and
`rustbinary` (both optional); `chrono`, `time` and `jiff` exist only here,
so the published crate stays free of third-party date/time libraries while
the benchmarks can still compare against them.

## What it measures

1. **Performance** — per-operation wall-clock timings (ns/op) for the
   equivalent operation in each library: RFC 3339 parse (Z and offset
   forms), RFC 3339 format, instant → civil projection, date + 1 day, date
   + 1 month (calendar-clamping), duration addition, weekday.
2. **Panic-freedom on adversarial input** — every library's parsers are fed
   a corpus of malformed strings plus thousands of deterministic fuzz
   strings under `catch_unwind`; the panic count is reported.
3. **Static facts** — overflow model, representable range, `no_std`
   support, MSRV, and the exact behavior of the pinned versions.

The GitHub Action (`.github/workflows/benchmark.yml`) runs this harness in
release mode, appends dependency-footprint and `unsafe`-usage counts for the
four libraries, and writes the assembled report to `benchmark.md` in the
repository root.

## Methodology

- Fixed, identical inputs for every library (same RFC 3339 strings, same
  date/time values).
- `black_box` on inputs and outputs; a warm-up loop before each measurement.
- Each operation is timed over a large fixed iteration count; the minimum of
  three runs is reported to reduce noise.
- Numbers are only comparable on the same machine/runner, not across
  machines. CI runs on `ubuntu-latest`.
- `tzcraft`'s allocator-free `write_rfc3339` is reported separately; the
  other three libraries only expose allocating format methods, so a direct
  comparison would be misleading.

## Run it

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml
```
