//! tzcraft vs chrono vs time vs jiff: performance + safety comparison.
//!
//! Development-only (package `tzcraft-benchmarks`, `publish = false`). The
//! three comparison libraries exist only in this package; they are never
//! part of `tzcraft`'s dependency graph.
//!
//! Output is a markdown report (stdout). The GitHub Action wraps it with a
//! header and dependency/`unsafe` footprint tables and writes `benchmark.md`.

use std::hint::black_box;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::str::FromStr;
use std::time::Instant;

use chrono::{Datelike, Timelike};
use time::format_description::well_known::Rfc3339;

// ---------------------------------------------------------------------------
// Timing
// ---------------------------------------------------------------------------

/// Time `f` over `iters` calls of a **fixed** input (used for parsing and
/// formatting, which are not loop-invariant-hoistable). Minimum of three
/// runs after a warm-up pass. Returns nanoseconds per operation.
fn bench_fixed(iters: u64, mut f: impl FnMut()) -> f64 {
    for _ in 0..10_000 {
        f();
    }
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        let ns = start.elapsed().as_nanos() as f64;
        best = best.min(ns / iters as f64);
    }
    best
}

/// Time `f(x)` over `iters` calls where `x` rotates through a **pre-built**
/// array of varied inputs. Varying the input defeats loop-invariant code
/// motion, so the measured cost is the operation itself, not a folded
/// constant. The input-array construction happens before timing.
fn bench_arr<T: Copy>(arr: &[T], iters: u64, mut f: impl FnMut(T)) -> f64 {
    let n = arr.len() as u64;
    for i in 0..10_000u64 {
        f(arr[(i % n) as usize]);
    }
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let start = Instant::now();
        for i in 0..iters {
            f(black_box(arr[(i % n) as usize]));
        }
        let ns = start.elapsed().as_nanos() as f64;
        best = best.min(ns / iters as f64);
    }
    best
}

/// Format one row of the performance table. `vals` is indexed
/// [tzcraft, chrono, time, jiff]; `None` means "not applicable".
fn fmt_row(name: &str, vals: [Option<f64>; 4]) -> String {
    let cells: Vec<String> = vals
        .iter()
        .map(|v| match v {
            Some(x) => format!("{x:.1}"),
            None => "—".to_string(),
        })
        .collect();
    let avail: Vec<f64> = vals.iter().flatten().copied().collect();
    let fastest = avail.iter().cloned().fold(f64::INFINITY, f64::min);
    let ratios: Vec<String> = vals
        .iter()
        .map(|v| match v {
            Some(x) => {
                if *x <= fastest * 1.0001 {
                    "**fastest**".to_string()
                } else {
                    format!("{:.2}×", x / fastest)
                }
            }
            None => "—".to_string(),
        })
        .collect();
    format!(
        "| {name} | {} | {} | {} | {} | {} |",
        cells[0], cells[1], cells[2], cells[3], ratios[0]
    )
}

// ---------------------------------------------------------------------------
// Performance measurements
// ---------------------------------------------------------------------------

const S_Z: &str = "2024-06-15T08:30:00.123456789Z";
const S_OFF: &str = "2024-06-15T08:30:00.123456789+08:00";

fn perf_section() -> String {
    let iters_heavy = 200_000u64;
    let iters_light = 1_000_000u64;

    // --- parse RFC 3339 (Z); fixed identical input ---
    let tz = bench_fixed(iters_heavy, || {
        black_box(tzcraft::Ticks::from_rfc3339(S_Z).unwrap());
    });
    let ch = bench_fixed(iters_heavy, || {
        black_box(S_Z.parse::<chrono::DateTime<chrono::Utc>>().unwrap());
    });
    let ti = bench_fixed(iters_heavy, || {
        black_box(time::OffsetDateTime::parse(S_Z, &Rfc3339).unwrap());
    });
    let ji = bench_fixed(iters_heavy, || {
        black_box(S_Z.parse::<jiff::Timestamp>().unwrap());
    });

    // --- parse RFC 3339 (with offset); fixed identical input ---
    let tz_off = bench_fixed(iters_heavy, || {
        black_box(tzcraft::Zoned::from_rfc3339(S_OFF).unwrap());
    });
    let ch_off = bench_fixed(iters_heavy, || {
        black_box(chrono::DateTime::parse_from_rfc3339(S_OFF).unwrap());
    });
    let ti_off = bench_fixed(iters_heavy, || {
        black_box(time::OffsetDateTime::parse(S_OFF, &Rfc3339).unwrap());
    });
    let ji_off = bench_fixed(iters_heavy, || {
        black_box(S_OFF.parse::<jiff::Timestamp>().unwrap());
    });

    // --- format RFC 3339 (allocating String, all four); fixed input ---
    let t = tzcraft::Ticks::from_rfc3339(S_Z).unwrap();
    let dt: chrono::DateTime<chrono::Utc> = S_Z.parse().unwrap();
    let odt = time::OffsetDateTime::parse(S_Z, &Rfc3339).unwrap();
    let ts: jiff::Timestamp = S_Z.parse().unwrap();

    let tz_fmt = bench_fixed(iters_heavy, || {
        black_box(t.to_rfc3339(tzcraft::FractionDigits::Auto));
    });
    let ch_fmt = bench_fixed(iters_heavy, || {
        black_box(dt.to_rfc3339());
    });
    let ti_fmt = bench_fixed(iters_heavy, || {
        black_box(odt.format(&Rfc3339).unwrap());
    });
    let ji_fmt = bench_fixed(iters_heavy, || {
        black_box(ts.to_string());
    });

    // --- format RFC 3339, allocator-free (tzcraft-only). The output slice
    // is black_box'd so the writes cannot be optimized away ---
    let tz_buf = bench_fixed(iters_heavy, || {
        let mut out = [0u8; 64];
        let n = t
            .write_rfc3339(&mut out, tzcraft::FractionDigits::Auto)
            .unwrap();
        black_box(&out[..n]);
    });

    // --- pre-built varied-input arrays (defeat loop-invariant code motion);
    // construction happens outside the timed region ---
    let tz_instants: Vec<tzcraft::Ticks> = (0..1024)
        .map(|i| {
            tzcraft::Ticks::from_unix_nanos(
                1_718_409_600_000_000_000 + (i as i128 % 1000) * 1_000_000 + i as i128,
            )
        })
        .collect();
    let ch_instants: Vec<chrono::DateTime<chrono::Utc>> = (0..1024)
        .map(|i| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(
                1_718_409_600 + (i as i64 % 1000),
                (i as u32 % 1000) * 1_000_000,
            )
            .unwrap()
        })
        .collect();
    let ti_instants: Vec<time::OffsetDateTime> = (0..1024)
        .map(|i| {
            time::OffsetDateTime::from_unix_timestamp_nanos(
                1_718_409_600_000_000_000 + (i as i128 % 1000) * 1_000_000 + i as i128,
            )
            .unwrap()
        })
        .collect();
    let ji_instants: Vec<jiff::Timestamp> = (0..1024)
        .map(|i| {
            jiff::Timestamp::from_nanosecond(
                1_718_409_600_000_000_000 + (i as i128 % 1000) * 1_000_000 + i as i128,
            )
            .unwrap()
        })
        .collect();

    let tz_dates: Vec<tzcraft::Date> = (0..1024)
        .map(|i| tzcraft::Date::from_days_since_epoch((i % 60_000) - 30_000))
        .collect();
    let ch_dates: Vec<chrono::NaiveDate> = (0..1024)
        .map(|i| {
            chrono::NaiveDate::from_num_days_from_ce_opt(
                (719_162 + (i as i64 % 60_000) - 30_000) as i32,
            )
            .unwrap()
        })
        .collect();
    let ti_dates: Vec<time::Date> = (0..1024)
        .map(|i| {
            time::Date::from_calendar_date(
                2024,
                time::Month::try_from((i % 12) as u8 + 1).unwrap(),
                (i % 28) as u8 + 1,
            )
            .unwrap()
        })
        .collect();
    let ji_dates: Vec<jiff::civil::Date> = (0..1024)
        .map(|i| jiff::civil::date(2024, ((i % 12) as i8) + 1, ((i % 28) as i8) + 1))
        .collect();

    let tz_durs: Vec<tzcraft::Duration> = (0..1024)
        .map(|i| tzcraft::Duration::from_nanos(i as i128 * 1_000_000 + 123))
        .collect();
    let ch_durs: Vec<chrono::TimeDelta> = (0..1024)
        .map(|i| chrono::TimeDelta::nanoseconds(i as i64 * 1_000_000 + 123))
        .collect();
    let ti_durs: Vec<time::Duration> = (0..1024)
        .map(|i| time::Duration::nanoseconds(i as i64 * 1_000_000 + 123))
        .collect();
    let ji_durs: Vec<jiff::Span> = (0..1024)
        .map(|i| jiff::Span::new().seconds(i as i64).nanoseconds(456_789_123))
        .collect();

    // --- instant → civil projection (varied input) ---
    let tz_proj = bench_arr(&tz_instants, iters_light, |x| {
        let c = x.to_civil_utc().unwrap();
        black_box(c.year() as i64 + c.month() as i64 + c.day() as i64 + c.hour() as i64);
    });
    let ch_proj = bench_arr(&ch_instants, iters_light, |x| {
        let n = x.naive_utc();
        black_box(
            n.date().year() as i64
                + n.date().month() as i64
                + n.date().day() as i64
                + n.time().hour() as i64,
        );
    });
    let ti_proj = bench_arr(&ti_instants, iters_light, |x| {
        black_box(x.year() as i64 + x.month() as i64 + x.day() as i64 + x.hour() as i64);
    });
    let ji_proj = bench_arr(&ji_instants, iters_light, |x| {
        let c = x.to_zoned(jiff::tz::TimeZone::UTC).datetime();
        black_box(c.year() as i64 + c.month() as i64 + c.day() as i64 + c.hour() as i64);
    });

    // --- date + 1 day (varied input) ---
    let tz_add_d = bench_arr(&tz_dates, iters_light, |x| {
        black_box(x.checked_add_days(tzcraft::Days::new(1)).unwrap());
    });
    let ch_add_d = bench_arr(&ch_dates, iters_light, |x| {
        black_box(x.checked_add_days(chrono::Days::new(1)).unwrap());
    });
    let ti_add_d = bench_arr(&ti_dates, iters_light, |x| {
        black_box(x.checked_add(time::Duration::days(1)).unwrap());
    });
    let ji_add_d = bench_arr(&ji_dates, iters_light, |x| {
        black_box(x.checked_add(jiff::Span::new().days(1)).unwrap());
    });

    // --- date + 1 month, calendar clamping (varied input) ---
    // `time` 0.3 has no month arithmetic on `Date` (its `Duration` is
    // day-precision only), so that cell is "not applicable".
    let tz_add_m = bench_arr(&tz_dates, iters_light, |x| {
        black_box(x.checked_add_months(tzcraft::Months::new(1)).unwrap());
    });
    let ch_add_m = bench_arr(&ch_dates, iters_light, |x| {
        black_box(x.checked_add_months(chrono::Months::new(1)).unwrap());
    });
    let ji_add_m = bench_arr(&ji_dates, iters_light, |x| {
        black_box(x.checked_add(jiff::Span::new().months(1)).unwrap());
    });

    // --- duration addition (varied input) ---
    let tz_dur_add = bench_arr(&tz_durs, iters_light, |x| {
        black_box(x.checked_add(tzcraft::Duration::from_seconds(90)).unwrap());
    });
    let ch_dur_add = bench_arr(&ch_durs, iters_light, |x| {
        black_box(x + chrono::TimeDelta::seconds(90));
    });
    let ti_dur_add = bench_arr(&ti_durs, iters_light, |x| {
        black_box(x + time::Duration::seconds(90));
    });
    let ji_dur_add = bench_arr(&ji_durs, iters_light, |x| {
        black_box(x.checked_add(jiff::Span::new().seconds(90)).unwrap());
    });

    // --- weekday (varied input) ---
    let tz_wd = bench_arr(&tz_dates, iters_light, |x| {
        black_box(x.weekday());
    });
    let ch_wd = bench_arr(&ch_dates, iters_light, |x| {
        black_box(x.weekday());
    });
    let ti_wd = bench_arr(&ti_dates, iters_light, |x| {
        black_box(x.weekday());
    });
    let ji_wd = bench_arr(&ji_dates, iters_light, |x| {
        black_box(x.weekday());
    });

    let mut out = String::new();
    out.push_str("### Performance (nanoseconds per operation; lower is better)\n\n");
    out.push_str("| operation | tzcraft | chrono | time | jiff | tzcraft vs fastest |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: |\n");
    out.push_str(&fmt_row(
        "parse RFC 3339 (Z)",
        [Some(tz), Some(ch), Some(ti), Some(ji)],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "parse RFC 3339 (+08:00)",
        [Some(tz_off), Some(ch_off), Some(ti_off), Some(ji_off)],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "format RFC 3339 (String)",
        [Some(tz_fmt), Some(ch_fmt), Some(ti_fmt), Some(ji_fmt)],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "format RFC 3339 (stack buffer)",
        [Some(tz_buf), None, None, None],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "instant → civil (y/m/d/h)",
        [Some(tz_proj), Some(ch_proj), Some(ti_proj), Some(ji_proj)],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "date + 1 day",
        [
            Some(tz_add_d),
            Some(ch_add_d),
            Some(ti_add_d),
            Some(ji_add_d),
        ],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "date + 1 month (clamping)",
        [Some(tz_add_m), Some(ch_add_m), None, Some(ji_add_m)],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "duration + 90 s",
        [
            Some(tz_dur_add),
            Some(ch_dur_add),
            Some(ti_dur_add),
            Some(ji_dur_add),
        ],
    ));
    out.push('\n');
    out.push_str(&fmt_row(
        "weekday",
        [Some(tz_wd), Some(ch_wd), Some(ti_wd), Some(ji_wd)],
    ));
    out.push('\n');
    out.push('\n');
    out.push_str("> Methodology: fixed identical inputs for parse/format; pre-built varied input arrays (indexed in a rotating loop) for the arithmetic/civil operations so loop-invariant code motion cannot fold them; `black_box` on inputs and outputs; 10k warm-up; minimum of 3 runs of a large fixed iteration count. Numbers are machine-specific (CI: `ubuntu-latest`) and only comparable within a single run.\n");
    out.push_str("> `time` 0.3's `Date` has no month arithmetic (its `Duration` is day-precision only), so that cell is not applicable.\n");
    out.push_str("> The `format RFC 3339 (stack buffer)` row is tzcraft's allocator-free `write_rfc3339` (no allocation); `chrono`/`time`/`jiff` expose only allocating format methods, so a direct comparison would be misleading.\n");
    out
}

// ---------------------------------------------------------------------------
// Panic-freedom on adversarial input
// ---------------------------------------------------------------------------

/// Deterministic xorshift64* — no external `rand` dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn ascii(&mut self, len: usize) -> String {
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            // Printable ASCII only (0x20..=0x7E), which is always valid
            // UTF-8 and includes every parser-relevant punctuation
            // (`-`, `:`, `.`, `T`, `Z`, `+`, `%`, ...).
            let b = (self.next() & 0x7F) as u8;
            bytes.push(if (0x20..=0x7E).contains(&b) { b } else { b' ' });
        }
        String::from_utf8(bytes).unwrap()
    }
}

fn fuzz_corpus() -> Vec<String> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut v: Vec<String> = vec![
        "".into(),
        "2024".into(),
        "2024-".into(),
        "2024-13-01".into(),
        "2024-00-01".into(),
        "2024-01-00".into(),
        "2024-02-30".into(),
        "2024-01-01T00:00:00".into(),
        "2024-01-01T24:00:00Z".into(),
        "2024-01-01T12:00:00+24:00".into(),
        "2024-01-01T12:00:00.1234567890Z".into(),
        "2024-01-01T12:00:00Z trailing".into(),
        "99999999-01-01".into(),
        "-2024-01-01".into(),
        "P1Y".into(),
        "PT".into(),
        "%%%%".into(),
        "%Y-%m-%d %Q".into(),
        "2024-W54-1".into(),
        "Mon, 1 Jan 2024 00:00:00 XYZ".into(),
    ];
    for len in [0usize, 1, 2, 4, 8, 16, 32, 64, 128] {
        for _ in 0..120 {
            v.push(rng.ascii(len));
        }
    }
    v
}

fn panic_count(inputs: &[String], f: impl Fn(&str)) -> usize {
    inputs
        .iter()
        .filter(|s| catch_unwind(AssertUnwindSafe(|| f(s))).is_err())
        .count()
}

fn safety_section() -> String {
    let corpus = fuzz_corpus();
    let n = corpus.len();

    let tz_panics = panic_count(&corpus, |s| {
        let _ = tzcraft::Ticks::from_rfc3339(s);
        let _ = tzcraft::Date::from_iso(s);
        let _ = tzcraft::TimeOfDay::from_iso(s);
        let _ = tzcraft::CivilDateTime::from_iso(s);
        let _ = tzcraft::Zoned::from_rfc3339(s);
        let _ = tzcraft::Offset::from_iso(s);
        let _ = tzcraft::Duration::from_iso8601(s);
        let _ = tzcraft::Ticks::from_rfc2822(s);
    });
    let ch_panics = panic_count(&corpus, |s| {
        let _ = s.parse::<chrono::DateTime<chrono::Utc>>();
        let _ = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d");
        let _ = chrono::NaiveTime::parse_from_str(s, "%H:%M:%S%.f");
    });
    let ti_panics = panic_count(&corpus, |s| {
        let _ = time::OffsetDateTime::parse(s, &Rfc3339);
        let _ = time::Date::parse(s, &time::format_description::well_known::Iso8601::DATE);
        let _ = time::Time::parse(s, &time::format_description::well_known::Iso8601::TIME);
    });
    let ji_panics = panic_count(&corpus, |s| {
        let _ = s.parse::<jiff::Timestamp>();
        let _ = jiff::civil::Date::from_str(s);
        let _ = jiff::civil::Time::from_str(s);
    });

    format!(
        "### Panic-freedom on adversarial input\n\n\
         Deterministic fuzz corpus of {n} strings (malformed dates, out-of-range components, \
         hostile format directives, random ASCII up to 128 bytes) fed to every library's parsers \
         under `catch_unwind`. A panic is a real finding.\n\n\
         | library | inputs | panics |\n\
         | --- | ---: | ---: |\n\
         | tzcraft | {n} | {tz_panics} |\n\
         | chrono | {n} | {ch_panics} |\n\
         | time | {n} | {ti_panics} |\n\
         | jiff | {n} | {ji_panics} |\n"
    )
}

// ---------------------------------------------------------------------------
// Static facts (verified against the pinned versions)
// ---------------------------------------------------------------------------

fn facts_section() -> String {
    "### Static facts (pinned versions)\n\n\
     | | tzcraft 0.1.1 | chrono 0.4.45 | time 0.3.55 | jiff 0.2.35 |\n\
     | --- | --- | --- | --- | --- |\n\
     | instant storage | `i128` ns since epoch | `i64` s + `u32` ns | `i64` ns of day + offset | `i64` s + `i64` ns |\n\
     | range | ≈ ±5.4×10^21 years | ≈ ±262,000 years | years 0000–9999 default (`large-dates` extends to ±999,999) | ≈ ±292 billion years (`i64` s) |\n\
     | Y2038-safe | yes (no 32-bit seconds) | yes | yes | yes |\n\
     | overflow model | `checked_*` returns `Result`; unit constructors compute in `i128` | `checked_*` returns `Option`/`Result`; `TimeDelta` is `i64` ns | `checked_*` returns `Result` | `checked_*` returns `Result` |\n\
     | `no_std` | yes, and **no allocator** with `--no-default-features` | yes, with `alloc` | yes, with `alloc` | yes, with `alloc` (std typical) |\n\
     | MSRV | 1.81 | 1.61 | 1.81 | 1.70 |\n\
     | unsafe in source | 0 (`#![deny(unsafe_code)]`) | see footprint below | see footprint below | see footprint below |\n\
     | IANA tz database | no | optional (via `iana-time-zone`) | no | bundled (tzdb) |\n\n\
     > MSRV values are read from each pinned crate's `rust-version`; range and storage facts are \
     from the public documentation of those exact versions.\n"
        .to_string()
}

fn main() {
    // Arguments: [output] [date] [commit] [runner]. The last three are the
    // CI metadata injected by the GitHub Action; when absent (local runs)
    // the header is simply shorter.
    let mut args = std::env::args().skip(1);
    let out_path = args.next().unwrap_or_else(|| "benchmark.md".to_string());
    let date = args.next();
    let commit = args.next();
    let runner = args.next();

    // Write the report to a file (UTF-8) so console code pages cannot mangle
    // it; the default path is `benchmark.md` in the current directory.
    let mut report = String::new();
    report.push_str("# tzcraft benchmark report\n\n");
    if let (Some(d), Some(c), Some(r)) = (&date, &commit, &runner) {
        report.push_str(&format!("- date: {d}\n- commit: {c}\n- runner: {r}\n\n"));
    }
    report.push_str(&format!(
        "- target: `{}-{}`\n",
        std::env::consts::ARCH,
        std::env::consts::OS
    ));
    report.push_str("\n## Results\n\n");
    report.push_str(&perf_section());
    report.push('\n');
    report.push_str(&safety_section());
    report.push('\n');
    report.push_str(&facts_section());

    std::fs::write(&out_path, report).expect("failed to write benchmark report");
    println!("benchmark report written to {out_path}");
}
