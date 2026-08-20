//! From-scratch ISO 8601 / RFC 3339 parsing and formatting.
//!
//! No regex, no `strftime`, no external parser. Formatting is hand-rolled
//! digit emission into a `String`; parsing is a byte scanner that reports
//! the exact byte offset of every failure. The accepted surface is strict
//! ISO 8601 extended format with a few deliberate conveniences:
//!
//! - fractional seconds of 1..9 digits (more is rejected, not truncated);
//! - offsets in `Z`, `±HH:MM`, `±HHMM`, `±HH` and `±HH:MM:SS`;
//! - `T`, `t` or a single space as the date/time separator;
//! - expanded years (optional sign, 4+ digits).

use crate::calendar::{NS_PER_DAY, NS_PER_HOUR, NS_PER_MIN, NS_PER_SEC};
use crate::date::Date;
use crate::duration::Duration;
use crate::error::{Error, Result};
use crate::offset::Offset;
use crate::time::TimeOfDay;
#[cfg(feature = "alloc")]
use crate::write::alloc_string;
use crate::write::{with_buf, write_padded, write_u128, Write};

#[cfg(feature = "alloc")]
use alloc::string::String;

/// Fractional-second precision for textual output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FractionDigits {
    /// No fractional part.
    None,
    /// Exactly three digits.
    Milli,
    /// Exactly six digits.
    Micro,
    /// Exactly nine digits.
    Nano,
    /// As many digits as needed, trailing zeros trimmed.
    Auto,
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// `YYYY-MM-DD`, with an expanded-year sign when needed.
pub(crate) fn format_date_into(out: &mut dyn Write, year: i32, month: u32, day: u32) -> Result<()> {
    if year < 0 {
        out.write_byte(b'-')?;
        write_padded(out, -(year as i64), 4)?;
    } else {
        write_padded(out, year as i64, 4)?;
    }
    out.write_byte(b'-')?;
    write_padded(out, month as i64, 2)?;
    out.write_byte(b'-')?;
    write_padded(out, day as i64, 2)?;
    Ok(())
}

/// `HH:MM:SS` plus the requested fractional part.
pub(crate) fn format_time_into(
    out: &mut dyn Write,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
    fraction: FractionDigits,
) -> Result<()> {
    write_padded(out, hour as i64, 2)?;
    out.write_byte(b':')?;
    write_padded(out, minute as i64, 2)?;
    out.write_byte(b':')?;
    write_padded(out, second as i64, 2)?;
    match fraction {
        FractionDigits::None => {}
        FractionDigits::Milli => {
            out.write_byte(b'.')?;
            write_padded(out, (nanos / 1_000_000) as i64, 3)?;
        }
        FractionDigits::Micro => {
            out.write_byte(b'.')?;
            write_padded(out, (nanos / 1_000) as i64, 6)?;
        }
        FractionDigits::Nano => {
            out.write_byte(b'.')?;
            write_padded(out, nanos as i64, 9)?;
        }
        FractionDigits::Auto => {
            if nanos != 0 {
                out.write_byte(b'.')?;
                let mut digits = [0u8; 9];
                let mut v = nanos;
                for i in (0..9).rev() {
                    digits[i] = (v % 10) as u8;
                    v /= 10;
                }
                let mut end = 9;
                while end > 0 && digits[end - 1] == 0 {
                    end -= 1;
                }
                for &d in &digits[..end] {
                    out.write_byte(b'0' + d)?;
                }
            }
        }
    }
    Ok(())
}

/// `Z` or `±HH:MM[:SS]` (seconds shown only when nonzero).
pub(crate) fn format_offset_into(out: &mut dyn Write, offset: Offset) -> Result<()> {
    if offset.is_utc() {
        out.write_byte(b'Z')?;
        return Ok(());
    }
    let secs = offset.as_seconds();
    out.write_byte(if secs < 0 { b'-' } else { b'+' })?;
    let abs = secs.unsigned_abs();
    let hours = abs / 3600;
    let rem = abs % 3600;
    let minutes = rem / 60;
    let seconds = rem % 60;
    write_padded(out, hours as i64, 2)?;
    out.write_byte(b':')?;
    write_padded(out, minutes as i64, 2)?;
    if seconds != 0 {
        out.write_byte(b':')?;
        write_padded(out, seconds as i64, 2)?;
    }
    Ok(())
}

/// `YYYY-MM-DDTHH:MM:SS[.fraction]offset`.
pub(crate) fn format_rfc3339_into(
    out: &mut dyn Write,
    date: Date,
    time: TimeOfDay,
    offset: Offset,
    fraction: FractionDigits,
) -> Result<()> {
    let (y, m, d) = date.parts();
    format_date_into(out, y, m, d)?;
    out.write_byte(b'T')?;
    let (h, mi, s, ns) = time.parts();
    format_time_into(out, h, mi, s, ns, fraction)?;
    format_offset_into(out, offset)
}

/// Render an RFC 3339 timestamp into a caller-owned buffer.
///
/// Returns the number of bytes written; the buffer must be large enough
/// (≈ 35 bytes for the longest form), otherwise [`Error::buffer_overflow`]
/// is returned. This is the allocator-free equivalent of
/// [`Ticks::to_rfc3339`](crate::Ticks::to_rfc3339).
pub(crate) fn write_rfc3339(
    out: &mut [u8],
    date: Date,
    time: TimeOfDay,
    offset: Offset,
    fraction: FractionDigits,
) -> Result<usize> {
    with_buf(out, |b| {
        format_rfc3339_into(b, date, time, offset, fraction)
    })
}

/// [`format_rfc3339_into`] for a [`Date`].
#[cfg(feature = "alloc")]
pub(crate) fn format_date(date: Date) -> String {
    let (y, m, d) = date.parts();
    alloc_string(|b| format_date_into(b, y, m, d)).expect("a 16-byte date fits 64 bytes")
}

/// [`format_time_into`] for a [`TimeOfDay`].
#[cfg(feature = "alloc")]
pub(crate) fn format_time(time: TimeOfDay, fraction: FractionDigits) -> String {
    let (h, m, s, ns) = time.parts();
    alloc_string(|b| format_time_into(b, h, m, s, ns, fraction))
        .expect("a 20-byte time fits 64 bytes")
}

/// Date plus time, no zone.
#[cfg(feature = "alloc")]
pub(crate) fn format_civil(dt: crate::datetime::CivilDateTime, fraction: FractionDigits) -> String {
    alloc_string(|b| {
        format_date_into(b, dt.year(), dt.month(), dt.day())?;
        b.write_byte(b'T')?;
        let (h, mi, s, ns) = dt.time().parts();
        format_time_into(b, h, mi, s, ns, fraction)
    })
    .expect("a 40-byte civil timestamp fits 64 bytes")
}

/// [`format_offset_into`] for an [`Offset`].
#[cfg(feature = "alloc")]
pub(crate) fn format_offset(offset: Offset) -> String {
    alloc_string(|b| format_offset_into(b, offset)).expect("a 10-byte offset fits 64 bytes")
}

/// RFC 3339 rendering to a `String` via a stack buffer (one allocation).
#[cfg(feature = "alloc")]
pub(crate) fn format_rfc3339_alloc(
    date: Date,
    time: TimeOfDay,
    offset: Offset,
    fraction: FractionDigits,
) -> Result<String> {
    alloc_string(|b| format_rfc3339_into(b, date, time, offset, fraction))
}

/// ISO 8601 duration rendering core.
///
/// Produces `P[n]DT[n]H[n]M[n]S` with an optional fractional second, or
/// `PT0S` for zero. Weeks are never produced (they canonicalize to days).
pub(crate) fn format_duration_iso_into(out: &mut dyn Write, d: Duration) -> Result<()> {
    if d.is_negative() {
        out.write_byte(b'-')?;
    }
    let abs = d.unsigned_abs();
    out.write_byte(b'P')?;

    let day = NS_PER_DAY as u128;
    let days = abs / day;
    let rem = abs % day;
    if days > 0 {
        write_u128(out, days)?;
        out.write_byte(b'D')?;
    }

    let hours = rem / (NS_PER_HOUR as u128);
    let rem = rem % (NS_PER_HOUR as u128);
    let minutes = rem / (NS_PER_MIN as u128);
    let rem = rem % (NS_PER_MIN as u128);
    let seconds = rem / (NS_PER_SEC as u128);
    let subsec = rem % (NS_PER_SEC as u128);

    let has_time = hours > 0 || minutes > 0 || seconds > 0 || subsec > 0;
    if has_time {
        out.write_byte(b'T')?;
        if hours > 0 {
            write_u128(out, hours)?;
            out.write_byte(b'H')?;
        }
        if minutes > 0 {
            write_u128(out, minutes)?;
            out.write_byte(b'M')?;
        }
        if seconds > 0 || subsec > 0 {
            write_u128(out, seconds)?;
            if subsec > 0 {
                out.write_byte(b'.')?;
                let mut digits = [0u8; 9];
                let mut v = subsec;
                for i in (0..9).rev() {
                    digits[i] = (v % 10) as u8;
                    v /= 10;
                }
                let mut end = 9;
                while end > 0 && digits[end - 1] == 0 {
                    end -= 1;
                }
                for &d in &digits[..end] {
                    out.write_byte(b'0' + d)?;
                }
            }
            out.write_byte(b'S')?;
        }
    }
    if !has_time && days == 0 {
        out.write_str("T0S")?;
    }
    Ok(())
}

/// Render an ISO 8601 duration into a caller-owned buffer.
pub(crate) fn write_duration_iso(out: &mut [u8], d: Duration) -> Result<usize> {
    with_buf(out, |b| format_duration_iso_into(b, d))
}

/// ISO 8601 duration rendering.
#[cfg(feature = "alloc")]
pub(crate) fn format_duration_iso(d: Duration) -> String {
    let mut out = String::new();
    format_duration_iso_into(&mut out, d).expect("a fresh String never overflows");
    out
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Byte scanner over a `&str` that reports failure positions.
///
/// Shared with the strftime engine (`strftime.rs`) so both parsers
/// report byte offsets identically.
pub(crate) struct Scanner<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(s: &'a str) -> Scanner<'a> {
        Scanner {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub(crate) fn bump(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    pub(crate) fn err<T>(&self, what: &'static str) -> Result<T> {
        Err(Error::parse(what, self.pos))
    }

    fn expect(&mut self, want: u8, what: &'static str) -> Result<()> {
        if self.peek() == Some(want) {
            self.pos += 1;
            Ok(())
        } else {
            self.err(what)
        }
    }

    /// Read `min..=max` ASCII digits as a `u64`, stopping at `max` digits.
    ///
    /// Fixed-width fields (month, hour, ...) must stop at their width even
    /// when the next byte is also a digit (e.g. the hour in `+0530`); the
    /// following delimiter check is what catches overflow like `2024-013-01`.
    fn digits(&mut self, min: usize, max: usize) -> Result<u64> {
        let mut v: u64 = 0;
        let mut n = 0usize;
        while n < max {
            match self.peek() {
                Some(b) if b.is_ascii_digit() => {
                    self.pos += 1;
                    v = v * 10 + (b - b'0') as u64;
                    n += 1;
                }
                _ => break,
            }
        }
        if n < min {
            return self.err("expected digits");
        }
        Ok(v)
    }

    /// Read zero or more ASCII digits (used by duration parsing), returning
    /// the value and the digit count so callers can distinguish an explicit
    /// `0` from a missing number.
    fn digits_opt(&mut self) -> Result<(u64, usize)> {
        let mut v: u64 = 0;
        let mut n = 0usize;
        while let Some(b) = self.peek() {
            if !b.is_ascii_digit() {
                break;
            }
            self.pos += 1;
            v = v * 10 + (b - b'0') as u64;
            n += 1;
            if n > 18 {
                return self.err("too many digits");
            }
        }
        Ok((v, n))
    }

    /// Read a `.digits` fraction returning `(scaled_value, digit_count)`.
    fn fraction(&mut self) -> Result<(u64, u32)> {
        self.expect(b'.', "expected '.'")?;
        let mut v: u64 = 0;
        let mut n = 0u32;
        while let Some(b) = self.peek() {
            if !b.is_ascii_digit() {
                break;
            }
            self.pos += 1;
            v = v * 10 + (b - b'0') as u64;
            n += 1;
            if n > 9 {
                return self.err("fraction has more than 9 digits");
            }
        }
        if n == 0 {
            return self.err("expected digits after '.'");
        }
        Ok((v, n))
    }
}

fn parse_date(sc: &mut Scanner<'_>) -> Result<(i32, u32, u32)> {
    let sign = match sc.peek() {
        Some(b'-') => {
            sc.pos += 1;
            -1i64
        }
        Some(b'+') => {
            sc.pos += 1;
            1i64
        }
        _ => 1i64,
    };
    let year = sc.digits(4, 9)? as i64 * sign;
    if year < i32::MIN as i64 || year > i32::MAX as i64 {
        return sc.err("year out of range");
    }
    sc.expect(b'-', "expected '-' after year")?;
    let month = sc.digits(2, 2)?;
    sc.expect(b'-', "expected '-' after month")?;
    let day = sc.digits(2, 2)?;
    Ok((year as i32, month as u32, day as u32))
}

fn parse_time(sc: &mut Scanner<'_>) -> Result<(u32, u32, u32, u32)> {
    let hour = sc.digits(2, 2)?;
    sc.expect(b':', "expected ':' after hour")?;
    let minute = sc.digits(2, 2)?;
    sc.expect(b':', "expected ':' after minute")?;
    let second = sc.digits(2, 2)?;
    let nanos = if sc.peek() == Some(b'.') {
        let (v, n) = sc.fraction()?;
        (v * 10u64.pow(9 - n)) as u32
    } else {
        0
    };
    Ok((hour as u32, minute as u32, second as u32, nanos))
}

fn parse_offset(sc: &mut Scanner<'_>) -> Result<Offset> {
    match sc.peek() {
        Some(b'Z') | Some(b'z') => {
            sc.pos += 1;
            Ok(Offset::UTC)
        }
        Some(b'+') | Some(b'-') => {
            let sign = if sc.bump() == Some(b'+') { 1i64 } else { -1i64 };
            let hours = sc.digits(2, 2)? as i64;
            let minutes = if sc.peek() == Some(b':') {
                sc.pos += 1;
                sc.digits(2, 2)? as i64
            } else if sc.peek().is_some_and(|b| b.is_ascii_digit()) {
                sc.digits(2, 2)? as i64
            } else {
                0
            };
            let seconds = if sc.peek() == Some(b':') {
                sc.pos += 1;
                sc.digits(2, 2)? as i64
            } else {
                0
            };
            if hours > 23 || minutes > 59 || seconds > 59 {
                return sc.err("offset component out of range");
            }
            let total = sign * (hours * 3600 + minutes * 60 + seconds);
            let total = i32::try_from(total).map_err(|_| Error::invalid_offset())?;
            Offset::from_seconds(total)
        }
        _ => sc.err("expected a timezone offset"),
    }
}

/// Parse an offset from a scanner, advancing it (shared with strftime `%z`).
pub(crate) fn scan_offset(sc: &mut Scanner<'_>) -> Result<Offset> {
    parse_offset(sc)
}

/// Result of scanning one ISO 8601 timestamp.
struct ScanResult {
    date: Date,
    time: Option<TimeOfDay>,
    offset: Option<Offset>,
}

/// Scan `s` as an ISO 8601 timestamp with configurable required sections.
fn scan_iso(s: &str, require_time: bool, require_offset: bool) -> Result<ScanResult> {
    let mut sc = Scanner::new(s);
    let (year, month, day) = parse_date(&mut sc)?;
    let date = Date::from_ymd(year, month, day)?;

    let mut time = None;
    match sc.peek() {
        Some(b'T') | Some(b't') | Some(b' ') => {
            if !require_time {
                return sc.err("time component not allowed");
            }
            sc.pos += 1;
            let (h, mi, s, ns) = parse_time(&mut sc)?;
            time = Some(TimeOfDay::from_hms_nano(h, mi, s, ns)?);
        }
        _ => {}
    }
    if require_time && time.is_none() {
        return sc.err("expected a time component");
    }

    let mut offset = None;
    match sc.peek() {
        Some(b'Z') | Some(b'z') | Some(b'+') | Some(b'-') => {
            if !require_offset {
                return sc.err("timezone offset not allowed");
            }
            offset = Some(parse_offset(&mut sc)?);
        }
        _ => {}
    }
    if require_offset && offset.is_none() {
        return sc.err("expected a timezone offset");
    }

    if !sc.at_end() {
        return sc.err("trailing characters");
    }
    Ok(ScanResult { date, time, offset })
}

/// Parse a calendar date only (`YYYY-MM-DD`).
pub(crate) fn parse_date_iso(s: &str) -> Result<Date> {
    Ok(scan_iso(s, false, false)?.date)
}

/// Parse a time of day only (`HH:MM:SS[.fraction]`).
pub(crate) fn parse_time_iso(s: &str) -> Result<TimeOfDay> {
    let mut sc = Scanner::new(s);
    let (h, m, s, ns) = parse_time(&mut sc)?;
    if !sc.at_end() {
        return sc.err("trailing characters");
    }
    TimeOfDay::from_hms_nano(h, m, s, ns)
}

/// Parse a local date-time (`YYYY-MM-DDTHH:MM:SS[.fraction]`, no zone).
pub(crate) fn parse_civil_iso(s: &str) -> Result<(Date, TimeOfDay)> {
    let r = scan_iso(s, true, false)?;
    Ok((r.date, r.time.expect("require_time enforced")))
}

/// Parse a full RFC 3339 timestamp (date, time, offset).
pub(crate) fn parse_rfc3339(s: &str) -> Result<(Date, TimeOfDay, Offset)> {
    let r = scan_iso(s, true, true)?;
    Ok((
        r.date,
        r.time.expect("require_time enforced"),
        r.offset.expect("require_offset enforced"),
    ))
}

/// Parse an offset in any accepted textual form.
pub(crate) fn parse_offset_iso(s: &str) -> Result<Offset> {
    let mut sc = Scanner::new(s);
    let offset = parse_offset(&mut sc)?;
    if !sc.at_end() {
        return sc.err("trailing characters");
    }
    Ok(offset)
}

// ---------------------------------------------------------------------------
// ISO 8601 durations
// ---------------------------------------------------------------------------

/// Parse an ISO 8601 duration.
///
/// Accepted grammar:
/// `[-]P[n]W`, `[-]P[n]DT[n]H[n]M[n]S` and `[-]P[n]D` variants, with a
/// fractional part allowed on the final component. Years (`Y`) and pre-`T`
/// months (`M`) are rejected as calendar-ambiguous.
pub(crate) fn parse_duration_iso(s: &str) -> Result<Duration> {
    let mut sc = Scanner::new(s);
    let negative = if sc.peek() == Some(b'-') {
        sc.pos += 1;
        true
    } else {
        false
    };
    sc.expect(b'P', "expected 'P'")?;

    let mut total: i128 = 0;
    let mut saw_component = false;
    let mut saw_t = false;
    let mut saw_week = false;
    let mut saw_day = false;

    // Pre-T components: W, D.
    loop {
        if sc.peek() == Some(b'T') {
            if saw_week {
                return sc.err("week duration cannot contain time components");
            }
            sc.pos += 1;
            saw_t = true;
            break;
        }
        if sc.at_end() {
            break;
        }
        let (int, frac, digits) = parse_duration_number(&mut sc)?;
        let (unit, is_week) = match sc.bump() {
            Some(b'W') if !saw_week && !saw_day => (7 * NS_PER_DAY, true),
            Some(b'D') if !saw_week && !saw_day => (NS_PER_DAY, false),
            Some(b'W') | Some(b'D') => return sc.err("duplicate or mixed date duration unit"),
            Some(_) => return sc.err("expected 'W' or 'D' before 'T'"),
            None => return sc.err("expected a unit after the number"),
        };
        total = add_duration_component(total, int, frac, digits, unit)?;
        saw_week = is_week;
        saw_day = !is_week;
        saw_component = true;
        if digits > 0 && !sc.at_end() {
            return sc.err("fraction is only allowed on the final component");
        }
    }

    // Post-T components: H, M, S. A `T` without a single time component is
    // malformed (`P1DT`), so track that the section actually produced one.
    let mut post_components = 0usize;
    let mut last_rank = 4u8;
    loop {
        if sc.at_end() {
            break;
        }
        let (int, frac, digits) = parse_duration_number(&mut sc)?;
        let (unit, rank) = match sc.bump() {
            Some(b'H') => (NS_PER_HOUR, 3),
            Some(b'M') => (NS_PER_MIN, 2),
            Some(b'S') => (NS_PER_SEC, 1),
            Some(b'W') | Some(b'D') => return sc.err("'W'/'D' are not allowed after 'T'"),
            Some(_) => return sc.err("expected 'H', 'M' or 'S' after 'T'"),
            None => return sc.err("expected a unit after the number"),
        };
        if rank >= last_rank {
            return sc.err("duplicate or out-of-order time duration unit");
        }
        last_rank = rank;
        total = add_duration_component(total, int, frac, digits, unit)?;
        saw_component = true;
        post_components += 1;
        if digits > 0 && !sc.at_end() {
            return sc.err("fraction is only allowed on the final component");
        }
    }

    if saw_t && post_components == 0 {
        return sc.err("expected a time component after 'T'");
    }
    if !sc.at_end() {
        return sc.err("trailing characters");
    }
    if !saw_component {
        return sc.err("duration has no components");
    }
    Ok(Duration::from_nanos(if negative { -total } else { total }))
}

/// Parse `int[.frac]`, returning `(int, scaled_frac, frac_digits)`.
///
/// An explicit zero integer (`PT0S`) is valid; a missing integer with no
/// fraction (`PTS`) is not.
fn parse_duration_number(sc: &mut Scanner<'_>) -> Result<(i128, i128, u32)> {
    let (int, int_digits) = sc.digits_opt()?;
    let (frac, digits) = if sc.peek() == Some(b'.') {
        sc.fraction()?
    } else {
        (0, 0)
    };
    if int_digits == 0 && frac == 0 {
        return sc.err("expected a number");
    }
    Ok((int as i128, frac as i128, digits))
}

/// Add `int * unit + frac * unit / 10^digits` to `total`, checked.
fn add_duration_component(
    total: i128,
    int: i128,
    frac: i128,
    digits: u32,
    unit: i128,
) -> Result<i128> {
    let mut v = total.checked_add(int * unit).ok_or_else(Error::overflow)?;
    if digits > 0 {
        let scale = 10i128.pow(digits);
        // Exact for <= 9 digits because every unit divides by 1e9 cleanly.
        let frac_ns = (frac * unit) / scale;
        v = v.checked_add(frac_ns).ok_or_else(Error::overflow)?;
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "alloc")]
    use crate::datetime::CivilDateTime;
    use crate::write::Buf;

    #[test]
    fn fraction_digits_variants() {
        let mut storage = [0u8; 64];
        let mut buf = Buf::new(&mut storage);
        format_time_into(&mut buf, 12, 0, 0, 500_000_000, FractionDigits::None).unwrap();
        assert_eq!(buf.as_str(), "12:00:00");
        buf.clear();
        format_time_into(&mut buf, 12, 0, 0, 500_000_000, FractionDigits::Milli).unwrap();
        assert_eq!(buf.as_str(), "12:00:00.500");
        buf.clear();
        format_time_into(&mut buf, 12, 0, 0, 500_000_000, FractionDigits::Micro).unwrap();
        assert_eq!(buf.as_str(), "12:00:00.500000");
        buf.clear();
        format_time_into(&mut buf, 12, 0, 0, 500_000_000, FractionDigits::Nano).unwrap();
        assert_eq!(buf.as_str(), "12:00:00.500000000");
        buf.clear();
        format_time_into(&mut buf, 12, 0, 0, 500_000_000, FractionDigits::Auto).unwrap();
        assert_eq!(buf.as_str(), "12:00:00.5");
    }

    #[test]
    fn negative_years_format() {
        let mut storage = [0u8; 64];
        let mut buf = Buf::new(&mut storage);
        format_date_into(&mut buf, -1, 12, 31).unwrap();
        assert_eq!(buf.as_str(), "-0001-12-31");
        buf.clear();
        format_date_into(&mut buf, 10_000, 1, 1).unwrap();
        assert_eq!(buf.as_str(), "10000-01-01");
    }

    #[test]
    fn buffer_overflow_is_an_error() {
        let mut small = [0u8; 4];
        let mut buf = Buf::new(&mut small);
        assert!(format_time_into(&mut buf, 23, 59, 59, 123_456_789, FractionDigits::Auto).is_err());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn offsets_with_seconds() {
        let o = Offset::from_hms(-4, 30, 15).unwrap();
        assert_eq!(format_offset(o), "-04:30:15");
        assert_eq!(format_offset(Offset::UTC), "Z");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn duration_format_vectors() {
        assert_eq!(Duration::ZERO.to_iso8601(), "PT0S");
        assert_eq!(Duration::from_seconds(90).to_iso8601(), "PT1M30S");
        assert_eq!(Duration::from_seconds(3600).to_iso8601(), "PT1H");
        assert_eq!(Duration::from_seconds(86_400).to_iso8601(), "P1D");
        assert_eq!(
            Duration::from_nanos(93_784_500_000_000).to_iso8601(),
            "P1DT2H3M4.5S"
        );
        assert_eq!(Duration::from_seconds(-1).to_iso8601(), "-PT1S");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn civil_format() {
        let dt = CivilDateTime::from_ymd_hms(2024, 2, 29, 23, 59, 59).unwrap();
        assert_eq!(
            format_civil(dt, FractionDigits::None),
            "2024-02-29T23:59:59"
        );
    }
}
