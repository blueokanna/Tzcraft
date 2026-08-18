//! Dependency-free micro-benchmarks for the hot paths.
//!
//! Run with `cargo run --release --example bench`. The numbers are wall-clock
//! timings of a fixed workload, printed as nanoseconds-per-operation; they
//! are meant to catch regressions on a given machine, not to be compared
//! across machines. CI runs this in release mode.

use std::hint::black_box;
use std::time::Instant;

use tzcraft::{CivilDateTime, Date, Duration, Ticks, TimeOfDay};

fn bench<F: FnMut()>(name: &str, iters: u64, mut f: F) {
    // Warm up the CPU caches and let the branch predictor settle.
    for _ in 0..10_000 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed().as_nanos();
    let per_op = elapsed / iters as u128;
    println!("{name:<40} {per_op:>8} ns/op");
}

fn main() {
    let d = Date::from_ymd(2024, 6, 15).unwrap();
    let t = TimeOfDay::from_hms_nano(8, 30, 0, 123_456_789).unwrap();
    let dt = CivilDateTime::new(d, t);
    let ticks = Ticks::from_rfc3339("2024-06-15T08:30:00.123456789Z").unwrap();
    let span = Duration::from_nanos(123_456_789_123);
    let mut out = [0u8; 64];

    bench("date civil projection (parts)", 1_000_000, || {
        black_box(black_box(d).parts());
    });
    bench("date weekday", 1_000_000, || {
        black_box(black_box(d).weekday());
    });
    bench("ticks -> civil utc", 1_000_000, || {
        let _ = black_box(black_box(ticks).to_civil_utc());
    });
    bench("ticks -> unix seconds", 1_000_000, || {
        let _ = black_box(black_box(ticks).to_unix_seconds());
    });
    bench("civil -> ticks utc", 1_000_000, || {
        let _ = black_box(black_box(dt).to_ticks_utc());
    });
    bench("ticks checked_add duration", 1_000_000, || {
        let _ = black_box(black_box(ticks).checked_add(black_box(span)));
    });
    bench("parse RFC 3339", 200_000, || {
        let _ = black_box(Ticks::from_rfc3339("2024-06-15T08:30:00.123456789Z"));
    });
    bench("format RFC 3339 (buffer)", 200_000, || {
        let _ = black_box(black_box(ticks).write_rfc3339(&mut out, tzcraft::FractionDigits::Auto));
    });
    bench("format strftime (buffer)", 200_000, || {
        let _ = black_box(black_box(ticks).write_format("%Y-%m-%d %H:%M:%S%.f", &mut out));
    });
    bench("parse ISO 8601 duration", 200_000, || {
        let _ = black_box(Duration::from_iso8601("P1DT2H3M4.5S"));
    });
}
