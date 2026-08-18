# tzcraft benchmark report

- date: 2026-08-18T06:12:27Z
- commit: 18d903d
- runner: Linux 6.17.0-1022-azure x86_64

- target: `x86_64-linux`

## Results

### Performance (nanoseconds per operation; lower is better)

| operation | tzcraft | chrono | time | jiff | tzcraft vs fastest |
| --- | ---: | ---: | ---: | ---: | ---: |
| parse RFC 3339 (Z) | 29.2 | 124.4 | 21.3 | 56.4 | 1.37× |
| parse RFC 3339 (+08:00) | 36.4 | 27.8 | 22.7 | 60.5 | 1.60× |
| format RFC 3339 (String) | 59.3 | 65.0 | 18.7 | 48.7 | 3.17× |
| format RFC 3339 (stack buffer) | 44.1 | — | — | — | **fastest** |
| instant → civil (y/m/d/h) | 18.6 | 2.1 | 4.0 | 4.9 | 8.97× |
| date + 1 day | 0.5 | 2.0 | 1.0 | 3.8 | **fastest** |
| date + 1 month (clamping) | 24.1 | 7.8 | — | 17.0 | 3.10× |
| duration + 90 s | 1.0 | 2.2 | 2.0 | 35.0 | **fastest** |
| weekday | 1.8 | 1.0 | 4.4 | 2.7 | 1.76× |

> Methodology: fixed identical inputs for parse/format; pre-built varied input arrays (indexed in a rotating loop) for the arithmetic/civil operations so loop-invariant code motion cannot fold them; `black_box` on inputs and outputs; 10k warm-up; minimum of 3 runs of a large fixed iteration count. Numbers are machine-specific (CI: `ubuntu-latest`) and only comparable within a single run.
> `time` 0.3's `Date` has no month arithmetic (its `Duration` is day-precision only), so that cell is not applicable.
> The `format RFC 3339 (stack buffer)` row is tzcraft's allocator-free `write_rfc3339` (no allocation); `chrono`/`time`/`jiff` expose only allocating format methods, so a direct comparison would be misleading.

### Panic-freedom on adversarial input

Deterministic fuzz corpus of 1100 strings (malformed dates, out-of-range components, hostile format directives, random ASCII up to 128 bytes) fed to every library's parsers under `catch_unwind`. A panic is a real finding.

| library | inputs | panics |
| --- | ---: | ---: |
| tzcraft | 1100 | 0 |
| chrono | 1100 | 0 |
| time | 1100 | 0 |
| jiff | 1100 | 0 |

### Static facts (pinned versions)

| | tzcraft 0.1.1 | chrono 0.4.45 | time 0.3.55 | jiff 0.2.35 |
| --- | --- | --- | --- | --- |
| instant storage | `i128` ns since epoch | `i64` s + `u32` ns | `i64` ns of day + offset | `i64` s + `i64` ns |
| range | ≈ ±292 billion years | ≈ ±262,000 years | years 0000–9999 default (`large-dates` extends to ±999,999) | ≈ ±292 billion years (`i64` ns) |
| Y2038-safe | yes (no 32-bit seconds) | yes | yes | yes |
| overflow model | `checked_*` returns `Result`; unit constructors compute in `i128` | `checked_*` returns `Option`/`Result`; `TimeDelta` is `i64` ns | `checked_*` returns `Result` | `checked_*` returns `Result` |
| `no_std` | yes, and **no allocator** with `--no-default-features` | yes, with `alloc` | yes, with `alloc` | yes, with `alloc` (std typical) |
| MSRV | 1.81 | 1.61 | 1.81 | 1.70 |
| unsafe in source | 0 (`#![deny(unsafe_code)]`) | see footprint below | see footprint below | see footprint below |
| IANA tz database | no | optional (via `iana-time-zone`) | no | bundled (tzdb) |

> MSRV values are read from each pinned crate's `rust-version`; range and storage facts are from the public documentation of those exact versions.

## Dependency footprint

| package | packages with `-e normal` (incl. itself) |
| --- | ---: |
| tzcraft | 5 |
| chrono | 2 |
| time | 5 |
| jiff | 2 |

> Counted with `cargo tree -p <pkg> -e normal --prefix none`. `tzcraft`'s graph here is as built for the benchmark (default features: codecs on). For a downstream consumer of the published crate the graph is **0 transitive packages**: `nextjson` and `rustbinary` are optional, codec-only dependencies.

## `unsafe` usage in crate source

| package | `unsafe` keyword occurrences |
| --- | ---: |
| tzcraft | 0 (`#![deny(unsafe_code)]`) |
| chrono | 12 |
| time | 274 |
| jiff | 79 |

> Raw count of the `unsafe` keyword across each crate's `.rs` sources as vendored in the cargo registry for the pinned versions. An `unsafe` count is a static signal, not a verdict: what matters is whether the unsafe is encapsulated, whether the soundness invariants are documented, and whether the public API is safe to call.
