//! strftime-style format strings, the `chrono`-compatible surface.
//!
//! This module is the answer to "can I replace `chrono`": the `%`-directive
//! formatting and parsing engine that `chrono` applications lean on. It is
//! written from scratch — no `strftime` libc call, no regex — on top of the
//! same byte scanner as the ISO parsers, so every failure carries a byte
//! offset and no input can panic or allocate without bound.
//!
//! Supported directives (formatting **and** parsing unless noted):
//!
//! | directive | meaning |
//! |---|---|
//! | `%Y %y %C` | year, 2-digit year, century |
//! | `%m %d %e` | month, day, space-padded day |
//! | `%j` | day of year (001-366) |
//! | `%H %I %k %l` | 24h hour, 12h hour, space-padded variants |
//! | `%M %S` | minute, second |
//! | `%f` `%.f` `%.3f` `%.6f` `%.9f` | nanoseconds, variable/fixed fraction |
//! | `%p %P` | AM/PM, am/pm |
//! | `%a %A %b %h %B` | weekday and month names |
//! | `%G %g %V` | ISO week year / ISO week number |
//! | `%u %w` | weekday numbers (ISO 1-7 / Sunday 0-6) |
//! | `%U %W` | week of year (Sunday-/Monday-based) |
//! | `%z %:z %Z` | offset `+HHMM`, `+HH:MM`, zone name |
//! | `%s` | Unix timestamp (seconds) |
//! | `%F %D %x %R %T %X %r` | composite forms |
//! | `%+` | RFC 3339 |
//! | `%n %t %%` | newline, tab, literal `%` |
//!
//! Padding modifiers `%-` (none), `%_` (space) and `%0` (zero) are honored,
//! and `%#S` formats seconds without a leading zero. Unlike `chrono`,
//! `format()` returns [`Result`] and rejects unknown directives instead of
//! silently emitting them — a deliberate safety choice.

use alloc::string::{String, ToString};

use crate::calendar::{
    Month, Weekday, days_from_civil, day_of_year, iso_week_from_civil, weekday_from_civil,
};
use crate::datetime::CivilDateTime;
use crate::date::Date;
use crate::duration::Duration;
use crate::error::{Error, Result};
use crate::format::{Scanner, parse_rfc3339, scan_offset};
use crate::offset::Offset;
use crate::ticks::Ticks;
use crate::time::TimeOfDay;
use crate::units::{Days, IsoWeek};
use crate::zone::Zone;
use crate::zoned::Zoned;

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// A fully-resolved view of a timestamp for the formatter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Parts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub nanos: u32,
    /// `None` means "no offset is shown" (civil/local form).
    pub offset: Option<Offset>,
    pub zone_name: Option<&'static str>,
    pub timestamp: Option<i64>,
}

/// Right-justify `v` to `width` with `pad` (`0`, `_` or space); `-` means no
/// padding.
fn pad_num(out: &mut String, v: u64, width: usize, pad: u8) {
    let s = v.to_string();
    if pad == b'-' || s.len() >= width {
        out.push_str(&s);
        return;
    }
    let padc = if pad == b'_' || pad == b' ' { b' ' } else { b'0' };
    for _ in s.len()..width {
        out.push(padc as char);
    }
    out.push_str(&s);
}

/// Emit a signed value with a leading `-` when negative.
fn emit_signed(out: &mut String, v: i64, width: usize, pad: u8) {
    if v < 0 {
        out.push('-');
    }
    pad_num(out, v.unsigned_abs(), width, pad);
}

fn hour12(hour: u32) -> u64 {
    let h = hour % 12;
    if h == 0 {
        12
    } else {
        h as u64
    }
}

/// Emit a fractional-second part. `width == 0` means variable (trim zeros).
fn emit_fraction(out: &mut String, nanos: u32, width: u32) {
    if width == 0 {
        if nanos != 0 {
            out.push('.');
            let mut digits = [0u8; 9];
            let mut v = nanos;
            for k in (0..9).rev() {
                digits[k] = (v % 10) as u8;
                v /= 10;
            }
            let mut end = 9;
            while end > 0 && digits[end - 1] == 0 {
                end -= 1;
            }
            for &d in &digits[..end] {
                out.push((b'0' + d) as char);
            }
        }
        return;
    }
    out.push('.');
    let scale = 9 - width;
    pad_num(out, (nanos / 10u32.pow(scale)) as u64, width as usize, b'0');
}

/// Week of year, Sunday-first (`%U`); week 0 covers days before the first Sunday.
fn week_of_year_sun(y: i32, m: u32, d: u32) -> u32 {
    let z = days_from_civil(y, m, d);
    let jan1 = days_from_civil(y, 1, 1);
    let jan1_sun = (weekday_from_civil(jan1) as i64 + 1) % 7;
    let first_sunday = jan1 + (7 - jan1_sun) % 7;
    let cur_sun = (weekday_from_civil(z) as i64 + 1) % 7;
    let week = (z - cur_sun - first_sunday) / 7 + 1;
    if week > 0 {
        week as u32
    } else {
        0
    }
}

/// Week of year, Monday-first (`%W`).
fn week_of_year_mon(y: i32, m: u32, d: u32) -> u32 {
    let z = days_from_civil(y, m, d);
    let jan1 = days_from_civil(y, 1, 1);
    let jan1_mon = weekday_from_civil(jan1) as i64;
    let first_monday = jan1 + (7 - jan1_mon) % 7;
    let cur_mon = weekday_from_civil(z) as i64;
    let week = (z - cur_mon - first_monday) / 7 + 1;
    if week > 0 {
        week as u32
    } else {
        0
    }
}

fn parse_width(bytes: &[u8]) -> Result<u32> {
    let text = core::str::from_utf8(bytes).map_err(|_| Error::invalid("format width"))?;
    text.parse().map_err(|_| Error::invalid("format width out of range"))
}

/// Format `p` according to the strftime-style `fmt` string.
pub(crate) fn format_parts(fmt: &str, p: &Parts) -> Result<String> {
    let mut out = String::new();
    let b = fmt.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c != b'%' {
            out.push(c as char);
            i += 1;
            continue;
        }
        i += 1;
        if i >= b.len() {
            return Err(Error::invalid("trailing '%' in format string"));
        }
        // `%:z` — colon-form offset.
        if b[i] == b':' {
            i += 1;
            if i >= b.len() || b[i] != b'z' {
                return Err(Error::invalid("expected 'z' after '%:'"));
            }
            i += 1;
            if let Some(off) = p.offset {
                push_offset(&mut out, off, true);
            }
            continue;
        }
        // `%.Nf` / `%.f` fractional directive.
        if b[i] == b'.' {
            i += 1;
            let mut width = 0u32;
            if i < b.len() && b[i].is_ascii_digit() {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                width = parse_width(&b[start..i])?;
                if width == 0 || width > 9 {
                    return Err(Error::invalid("fraction width must be 1..=9"));
                }
            }
            if i >= b.len() || b[i] != b'f' {
                return Err(Error::invalid("expected 'f' after '.' in format string"));
            }
            i += 1;
            emit_fraction(&mut out, p.nanos, width);
            continue;
        }
        // Padding modifier: `-`, `_`, `0`, `#`.
        let mut pad = b'0';
        if matches!(b[i], b'-' | b'_' | b'0' | b'#') {
            if b[i] == b'#' {
                pad = b'-';
            } else {
                pad = b[i];
            }
            i += 1;
        }
        // Optional numeric width (used by `%Y`).
        let mut width = 0usize;
        if i < b.len() && b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            width = parse_width(&b[start..i])? as usize;
        }
        if i >= b.len() {
            return Err(Error::invalid("trailing '%' in format string"));
        }
        let d = b[i];
        i += 1;
        emit_directive(&mut out, d, pad, width, p)?;
    }
    Ok(out)
}

fn emit_directive(out: &mut String, d: u8, pad: u8, width: usize, p: &Parts) -> Result<()> {
    let year = p.year as i64;
    let wd = weekday_from_civil(days_from_civil(p.year, p.month, p.day));
    let (iso_year, iso_week) = iso_week_from_civil(p.year, p.month, p.day);
    match d {
        b'Y' => emit_signed(out, year, width.max(4), pad),
        b'y' => pad_num(out, year.rem_euclid(100) as u64, 2, pad),
        b'C' => pad_num(out, year.div_euclid(100) as u64, 2, pad),
        b'm' => pad_num(out, p.month as u64, 2, pad),
        b'd' => pad_num(out, p.day as u64, 2, pad),
        b'e' => pad_num(out, p.day as u64, 2, b' '),
        b'j' => pad_num(out, day_of_year(p.year, p.month, p.day) as u64, 3, pad),
        b'H' => pad_num(out, p.hour as u64, 2, pad),
        b'I' => pad_num(out, hour12(p.hour), 2, pad),
        b'k' => pad_num(out, p.hour as u64, 2, b' '),
        b'l' => pad_num(out, hour12(p.hour), 2, b' '),
        b'M' => pad_num(out, p.minute as u64, 2, pad),
        b'S' => pad_num(out, p.second as u64, 2, pad),
        b'f' => pad_num(out, p.nanos as u64, 9, b'0'),
        b'p' => out.push_str(if p.hour < 12 { "AM" } else { "PM" }),
        b'P' => out.push_str(if p.hour < 12 { "am" } else { "pm" }),
        b'a' => out.push_str(wd.short_name()),
        b'A' => out.push_str(wd.name()),
        b'b' | b'h' => out.push_str(Month::from_u32(p.month).map_or("", |m| m.short_name())),
        b'B' => out.push_str(Month::from_u32(p.month).map_or("", |m| m.name())),
        b'G' => emit_signed(out, iso_year as i64, width.max(4), pad),
        b'g' => pad_num(out, (iso_year as i64).rem_euclid(100) as u64, 2, pad),
        b'V' => pad_num(out, iso_week as u64, 2, pad),
        b'u' => pad_num(out, (wd as u8 + 1) as u64, 1, pad),
        b'w' => pad_num(out, ((wd as u8 + 1) % 7) as u64, 1, pad),
        b'U' => pad_num(out, week_of_year_sun(p.year, p.month, p.day) as u64, 2, pad),
        b'W' => pad_num(out, week_of_year_mon(p.year, p.month, p.day) as u64, 2, pad),
        b'z' => {
            if let Some(off) = p.offset {
                push_offset(out, off, false);
            }
        }
        b'Z' => {
            if let Some(name) = p.zone_name {
                out.push_str(name);
            }
        }
        b's' => {
            if let Some(ts) = p.timestamp {
                out.push_str(&ts.to_string());
            }
        }
        b'n' => out.push('\n'),
        b't' => out.push('\t'),
        b'%' => out.push('%'),
        b'F' => {
            emit_directive(out, b'Y', pad, width, p)?;
            out.push('-');
            emit_directive(out, b'm', pad, width, p)?;
            out.push('-');
            emit_directive(out, b'd', pad, width, p)?;
        }
        b'D' | b'x' => {
            emit_directive(out, b'm', pad, width, p)?;
            out.push('/');
            emit_directive(out, b'd', pad, width, p)?;
            out.push('/');
            emit_directive(out, b'y', pad, width, p)?;
        }
        b'R' => {
            emit_directive(out, b'H', pad, width, p)?;
            out.push(':');
            emit_directive(out, b'M', pad, width, p)?;
        }
        b'T' | b'X' => {
            emit_directive(out, b'H', pad, width, p)?;
            out.push(':');
            emit_directive(out, b'M', pad, width, p)?;
            out.push(':');
            emit_directive(out, b'S', pad, width, p)?;
        }
        b'r' => {
            emit_directive(out, b'I', pad, width, p)?;
            out.push(':');
            emit_directive(out, b'M', pad, width, p)?;
            out.push(':');
            emit_directive(out, b'S', pad, width, p)?;
            out.push(' ');
            emit_directive(out, b'p', pad, width, p)?;
        }
        b'+' => {
            emit_directive(out, b'Y', pad, 4, p)?;
            out.push('-');
            emit_directive(out, b'm', pad, 2, p)?;
            out.push('-');
            emit_directive(out, b'd', pad, 2, p)?;
            out.push('T');
            emit_directive(out, b'H', pad, 2, p)?;
            out.push(':');
            emit_directive(out, b'M', pad, 2, p)?;
            out.push(':');
            emit_directive(out, b'S', pad, 2, p)?;
            if p.nanos != 0 {
                emit_fraction(out, p.nanos, 0);
            }
            match p.offset {
                Some(off) => push_offset(out, off, true),
                None => out.push('Z'),
            }
        }
        _ => return Err(Error::invalid("unknown format directive")),
    }
    Ok(())
}

fn push_offset(out: &mut String, offset: Offset, with_colon: bool) {
    if offset.is_utc() {
        if with_colon {
            out.push_str("+00:00");
        } else {
            out.push_str("+0000");
        }
        return;
    }
    let secs = offset.as_seconds();
    out.push(if secs < 0 { '-' } else { '+' });
    let abs = secs.unsigned_abs();
    let hours = abs / 3600;
    let rem = abs % 3600;
    let minutes = rem / 60;
    let seconds = rem % 60;
    if with_colon {
        let mut tmp = String::new();
        pad_num(&mut tmp, hours as u64, 2, b'0');
        out.push_str(&tmp);
        out.push(':');
        let mut tmp = String::new();
        pad_num(&mut tmp, minutes as u64, 2, b'0');
        out.push_str(&tmp);
    } else {
        let mut tmp = String::new();
        pad_num(&mut tmp, hours as u64, 2, b'0');
        out.push_str(&tmp);
        let mut tmp = String::new();
        pad_num(&mut tmp, minutes as u64, 2, b'0');
        out.push_str(&tmp);
    }
    if seconds != 0 {
        if with_colon {
            out.push(':');
        }
        let mut tmp = String::new();
        pad_num(&mut tmp, seconds as u64, 2, b'0');
        out.push_str(&tmp);
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Accumulated fields from a strftime parse.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ParsedParts {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
    pub hour: Option<u32>,
    pub minute: Option<u32>,
    pub second: Option<u32>,
    pub nanos: Option<u32>,
    pub ordinal: Option<u32>,
    pub isoyear: Option<i32>,
    pub iso_year2: Option<u32>,
    pub isoweek: Option<u32>,
    pub weekday: Option<Weekday>,
    pub week_sun: Option<u32>,
    pub week_mon: Option<u32>,
    pub hour12: Option<u32>,
    pub ampm: Option<bool>,
    pub offset: Option<Offset>,
    pub timestamp: Option<i64>,
    pub century: Option<i32>,
    pub year2: Option<u32>,
}

impl ParsedParts {
    fn empty() -> ParsedParts {
        ParsedParts {
            year: None,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
            nanos: None,
            ordinal: None,
            isoyear: None,
            iso_year2: None,
            isoweek: None,
            weekday: None,
            week_sun: None,
            week_mon: None,
            hour12: None,
            ampm: None,
            offset: None,
            timestamp: None,
            century: None,
            year2: None,
        }
    }

    /// Resolve the calendar year from `%Y` or `%C`+`%y` / `%y`.
    fn combine_year(self) -> Result<Option<i32>> {
        if let Some(y) = self.year {
            return Ok(Some(y));
        }
        match (self.century, self.year2) {
            (Some(c), Some(y2)) => Ok(Some(c * 100 + y2 as i32)),
            (None, Some(y2)) => {
                // POSIX century inference: 69-99 -> 19xx, 00-68 -> 20xx.
                Ok(Some(if y2 >= 69 { 1900 + y2 as i32 } else { 2000 + y2 as i32 }))
            }
            (Some(_), None) => Err(Error::invalid("incomplete year (missing %y)")),
            (None, None) => Ok(None),
        }
    }

    /// Resolve the hour, converting a 12-hour clock with AM/PM when needed.
    fn resolve_hour(self) -> Result<Option<u32>> {
        if let Some(h) = self.hour {
            return Ok(Some(h));
        }
        if let Some(h12) = self.hour12 {
            let pm = self
                .ampm
                .ok_or_else(|| Error::invalid("12-hour clock requires %p"))?;
            let h = if pm {
                if h12 == 12 {
                    12
                } else {
                    h12 + 12
                }
            } else if h12 == 12 {
                0
            } else {
                h12
            };
            return Ok(Some(h));
        }
        Ok(None)
    }

    fn has_any_time(self) -> bool {
        self.hour.is_some()
            || self.hour12.is_some()
            || self.minute.is_some()
            || self.second.is_some()
            || self.nanos.is_some()
            || self.ampm.is_some()
    }

    /// Resolve the ISO year from `%G` or the 2-digit `%g` (with the same
    /// POSIX century inference as `%y`).
    fn iso_year(self) -> Result<Option<i32>> {
        if let Some(y) = self.isoyear {
            return Ok(Some(y));
        }
        Ok(self.iso_year2.map(|y2| if y2 >= 69 { 1900 + y2 as i32 } else { 2000 + y2 as i32 }))
    }

    /// Resolve to a calendar date (calendar, ordinal, ISO-week or week-based).
    pub(crate) fn to_date(self) -> Result<Date> {
        if self.timestamp.is_some() {
            return Err(Error::invalid("a timestamp cannot become a calendar date"));
        }
        // ISO week date: %G %V %u.
        if let Some(iw) = self.isoweek {
            let iy = self
                .iso_year()?
                .ok_or_else(|| Error::invalid("ISO week date requires an ISO year (%G)"))?;
            let wd = self
                .weekday
                .ok_or_else(|| Error::invalid("ISO week date requires a weekday (%u)"))?;
            let monday = IsoWeek::new(iy, iw).monday()?;
            let date = monday.checked_add_days(Days::new(wd as u64))?;
            let (yy, ww) = date.iso_week().parts();
            if yy != iy || ww != iw {
                return Err(Error::invalid("ISO week date out of range"));
            }
            return Ok(date);
        }
        // Week-of-year date: %U/%W with a weekday.
        if let Some(week) = self.week_sun.or(self.week_mon) {
            let year = self
                .combine_year()?
                .ok_or_else(|| Error::invalid("week date requires a year"))?;
            let wd = self
                .weekday
                .ok_or_else(|| Error::invalid("week date requires a weekday"))?;
            return date_from_week(year, week, wd, self.week_sun.is_some());
        }
        let year = self
            .combine_year()?
            .ok_or_else(|| Error::invalid("missing year"))?;
        if let Some(ord) = self.ordinal {
            return check_weekday(self.weekday, Date::from_yo(year, ord)?);
        }
        if let (Some(m), Some(d)) = (self.month, self.day) {
            return check_weekday(self.weekday, Date::from_ymd(year, m, d)?);
        }
        Err(Error::invalid("incomplete date"))
    }

    /// Resolve to a time of day (missing sub-fields default to zero).
    pub(crate) fn to_time(self) -> Result<TimeOfDay> {
        if self.timestamp.is_some() {
            return Err(Error::invalid("a timestamp cannot become a time of day"));
        }
        let hour = self
            .resolve_hour()?
            .ok_or_else(|| Error::invalid("missing hour"))?;
        if hour >= 24 {
            return Err(Error::out_of_range("hour"));
        }
        TimeOfDay::from_hms_nano(
            hour,
            self.minute.unwrap_or(0),
            self.second.unwrap_or(0),
            self.nanos.unwrap_or(0),
        )
    }

    /// Resolve to a civil date-time (time defaults to midnight).
    pub(crate) fn to_civil(self) -> Result<(Date, TimeOfDay)> {
        let date = self.to_date()?;
        let time = if self.has_any_time() {
            self.to_time()?
        } else {
            TimeOfDay::MIDNIGHT
        };
        Ok((date, time))
    }

    /// Resolve to UTC ticks; the civil path requires a timezone offset.
    pub(crate) fn to_ticks(self) -> Result<Ticks> {
        if let Some(ts) = self.timestamp {
            return Ticks::from_timestamp(ts, 0);
        }
        let (date, time) = self.to_civil()?;
        let offset = self
            .offset
            .ok_or_else(|| Error::invalid("missing timezone offset"))?;
        let utc = CivilDateTime::new(date, time).to_ticks_utc()?;
        utc.checked_sub(Duration::from_seconds(offset.as_seconds() as i64))
    }
}

fn check_weekday(weekday: Option<Weekday>, date: Date) -> Result<Date> {
    if let Some(wd) = weekday {
        if date.weekday() != wd {
            return Err(Error::invalid("weekday does not match the date"));
        }
    }
    Ok(date)
}

/// Date from a week-of-year number (`%U` Sunday-first, `%W` Monday-first).
fn date_from_week(year: i32, week: u32, wd: Weekday, sunday_based: bool) -> Result<Date> {
    if week > 53 {
        return Err(Error::out_of_range("week"));
    }
    let jan1 = days_from_civil(year, 1, 1);
    let first = if sunday_based {
        let jan1_sun = (weekday_from_civil(jan1) as i64 + 1) % 7;
        jan1 + (7 - jan1_sun) % 7
    } else {
        let jan1_mon = weekday_from_civil(jan1) as i64;
        jan1 + (7 - jan1_mon) % 7
    };
    let wd_idx = if sunday_based {
        (wd as i64 + 1) % 7
    } else {
        wd as i64
    };
    let z = first + (week as i64 - 1) * 7 + wd_idx;
    let date = Date::from_days_checked(z)?;
    if date.year() != year {
        return Err(Error::invalid("week date spills into another year"));
    }
    Ok(date)
}

/// Read `min..=max` digits, allowing leading spaces when `pad` is a space.
fn read_int(sc: &mut Scanner<'_>, max: usize, pad: u8) -> Result<u64> {
    if pad == b'_' || pad == b' ' {
        while sc.peek() == Some(b' ') {
            sc.pos += 1;
        }
    }
    let mut v: u64 = 0;
    let mut n = 0usize;
    while n < max {
        match sc.peek() {
            Some(b) if b.is_ascii_digit() => {
                sc.pos += 1;
                v = v * 10 + (b - b'0') as u64;
                n += 1;
            }
            _ => break,
        }
    }
    if n == 0 {
        return sc.err("expected digits");
    }
    Ok(v)
}

/// Read exactly `n` digits.
fn read_int_exact(sc: &mut Scanner<'_>, n: usize) -> Result<u64> {
    let mut v: u64 = 0;
    for _ in 0..n {
        match sc.peek() {
            Some(b) if b.is_ascii_digit() => {
                sc.pos += 1;
                v = v * 10 + (b - b'0') as u64;
            }
            _ => return sc.err("expected digits"),
        }
    }
    Ok(v)
}

/// Consume `word` case-insensitively; `true` when matched.
fn eat_ci(sc: &mut Scanner<'_>, word: &str) -> bool {
    let wb = word.as_bytes();
    if sc.bytes.len() - sc.pos < wb.len() {
        return false;
    }
    for &w in wb {
        let c = sc.bytes[sc.pos];
        if !c.eq_ignore_ascii_case(&w) {
            return false;
        }
        sc.pos += 1;
    }
    true
}

/// Parse a fractional part: `.digits` scaled to nanoseconds.
fn parse_fraction(sc: &mut Scanner<'_>, width: u32) -> Result<u32> {
    if sc.peek() != Some(b'.') {
        return sc.err("expected '.'");
    }
    sc.pos += 1;
    let (v, n) = if width > 0 {
        let mut v = 0u64;
        let mut k = 0u32;
        while k < width {
            match sc.peek() {
                Some(b) if b.is_ascii_digit() => {
                    sc.pos += 1;
                    v = v * 10 + (b - b'0') as u64;
                    k += 1;
                }
                _ => return sc.err("expected digits"),
            }
        }
        (v, width)
    } else {
        let mut v = 0u64;
        let mut k = 0u32;
        while let Some(b) = sc.peek() {
            if !b.is_ascii_digit() {
                break;
            }
            sc.pos += 1;
            v = v * 10 + (b - b'0') as u64;
            k += 1;
            if k > 9 {
                return sc.err("fraction has more than 9 digits");
            }
        }
        if k == 0 {
            return sc.err("expected digits after '.'");
        }
        (v, k)
    };
    let ns = v * 10u64.pow(9 - n);
    if ns > u32::MAX as u64 {
        return sc.err("fraction out of range");
    }
    Ok(ns as u32)
}

/// Parse a weekday name (full or 3-letter).
fn parse_weekday_name(sc: &mut Scanner<'_>) -> Result<Option<Weekday>> {
    let start = sc.pos;
    while sc.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
        sc.pos += 1;
    }
    if sc.pos == start {
        return Ok(None);
    }
    let word = core::str::from_utf8(&sc.bytes[start..sc.pos])
        .map_err(|_| Error::invalid("invalid utf-8 in input"))?;
    if let Some(w) = Weekday::from_name(word) {
        return Ok(Some(w));
    }
    if let Some(w) = Weekday::from_short_name(word) {
        return Ok(Some(w));
    }
    Err(Error::invalid("unknown weekday name"))
}

/// Parse a month name (full or 3-letter), case-insensitive.
fn parse_month_name(sc: &mut Scanner<'_>) -> Result<Option<Month>> {
    let start = sc.pos;
    while sc.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
        sc.pos += 1;
    }
    if sc.pos == start {
        return Ok(None);
    }
    let word = core::str::from_utf8(&sc.bytes[start..sc.pos])
        .map_err(|_| Error::invalid("invalid utf-8 in input"))?;
    for m in [
        Month::January,
        Month::February,
        Month::March,
        Month::April,
        Month::May,
        Month::June,
        Month::July,
        Month::August,
        Month::September,
        Month::October,
        Month::November,
        Month::December,
    ] {
        if word.eq_ignore_ascii_case(m.name()) || word.eq_ignore_ascii_case(m.short_name()) {
            return Ok(Some(m));
        }
    }
    Err(Error::invalid("unknown month name"))
}

fn weekday_from_sunday(n: u64) -> Option<Weekday> {
    match n {
        0 => Some(Weekday::Sunday),
        1 => Some(Weekday::Monday),
        2 => Some(Weekday::Tuesday),
        3 => Some(Weekday::Wednesday),
        4 => Some(Weekday::Thursday),
        5 => Some(Weekday::Friday),
        6 => Some(Weekday::Saturday),
        _ => None,
    }
}

/// Parse `s` against the strftime-style `fmt`, accumulating fields.
pub(crate) fn parse_parts(fmt: &str, s: &str) -> Result<ParsedParts> {
    let mut pp = ParsedParts::empty();
    let mut sc = Scanner::new(s);
    let fb = fmt.as_bytes();
    let mut i = 0usize;
    while i < fb.len() {
        let c = fb[i];
        if c != b'%' {
            if c == b' ' {
                while matches!(sc.peek(), Some(b' ') | Some(b'\t')) {
                    sc.pos += 1;
                }
            } else if sc.peek() != Some(c) {
                return sc.err("expected literal character");
            } else {
                sc.pos += 1;
            }
            i += 1;
            continue;
        }
        i += 1;
        if i >= fb.len() {
            return Err(Error::invalid("trailing '%' in format string"));
        }
        // `%:z` — colon-form offset.
        if fb[i] == b':' {
            i += 1;
            if i >= fb.len() || fb[i] != b'z' {
                return Err(Error::invalid("expected 'z' after '%:'"));
            }
            i += 1;
            pp.offset = Some(scan_offset(&mut sc)?);
            continue;
        }
        if fb[i] == b'.' {
            i += 1;
            let mut width = 0u32;
            if i < fb.len() && fb[i].is_ascii_digit() {
                let start = i;
                while i < fb.len() && fb[i].is_ascii_digit() {
                    i += 1;
                }
                width = parse_width(&fb[start..i])?;
                if width == 0 || width > 9 {
                    return Err(Error::invalid("fraction width must be 1..=9"));
                }
            }
            if i >= fb.len() || fb[i] != b'f' {
                return Err(Error::invalid("expected 'f' after '.' in format string"));
            }
            i += 1;
            pp.nanos = Some(parse_fraction(&mut sc, width)?);
            continue;
        }
        let mut pad = b'0';
        if matches!(fb[i], b'-' | b'_' | b'0' | b'#') {
            pad = if fb[i] == b'#' { b'-' } else { fb[i] };
            i += 1;
        }
        let mut width = 0u32;
        if i < fb.len() && fb[i].is_ascii_digit() {
            let start = i;
            while i < fb.len() && fb[i].is_ascii_digit() {
                i += 1;
            }
            width = parse_width(&fb[start..i])?;
        }
        if i >= fb.len() {
            return Err(Error::invalid("trailing '%' in format string"));
        }
        let d = fb[i];
        i += 1;
        parse_directive(&mut sc, &mut pp, d, pad, width)?;
    }
    if !sc.at_end() {
        return sc.err("trailing characters in input");
    }
    Ok(pp)
}

fn parse_directive(
    sc: &mut Scanner<'_>,
    pp: &mut ParsedParts,
    d: u8,
    pad: u8,
    _width: u32,
) -> Result<()> {
    match d {
        b'Y' => {
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
            let v = read_int(sc, 9, pad)? as i64 * sign;
            if v < i32::MIN as i64 || v > i32::MAX as i64 {
                return sc.err("year out of range");
            }
            pp.year = Some(v as i32);
        }
        b'y' => pp.year2 = Some(read_int(sc, 2, pad)? as u32),
        b'C' => pp.century = Some(read_int(sc, 2, pad)? as i32),
        b'm' => pp.month = Some(read_int(sc, 2, pad)? as u32),
        b'd' => pp.day = Some(read_int(sc, 2, pad)? as u32),
        b'e' => pp.day = Some(read_int(sc, 2, b' ')? as u32),
        b'j' => pp.ordinal = Some(read_int(sc, 3, pad)? as u32),
        b'H' => pp.hour = Some(read_int(sc, 2, pad)? as u32),
        b'I' => pp.hour12 = Some(read_int(sc, 2, pad)? as u32),
        b'k' => pp.hour = Some(read_int(sc, 2, b' ')? as u32),
        b'l' => pp.hour12 = Some(read_int(sc, 2, b' ')? as u32),
        b'M' => pp.minute = Some(read_int(sc, 2, pad)? as u32),
        b'S' => pp.second = Some(read_int(sc, 2, pad)? as u32),
        b'f' => pp.nanos = Some(read_int_exact(sc, 9)? as u32),
        b'p' | b'P' => {
            if eat_ci(sc, "am") {
                pp.ampm = Some(false);
            } else if eat_ci(sc, "pm") {
                pp.ampm = Some(true);
            } else {
                return sc.err("expected AM/PM");
            }
        }
        b'a' | b'A' => {
            pp.weekday = parse_weekday_name(sc)?;
        }
        b'b' | b'h' | b'B' => {
            pp.month = parse_month_name(sc)?.map(|m| m.as_u32());
        }
        b'G' => {
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
            let v = read_int(sc, 9, pad)? as i64 * sign;
            if v < i32::MIN as i64 || v > i32::MAX as i64 {
                return sc.err("iso year out of range");
            }
            pp.isoyear = Some(v as i32);
        }
        b'g' => pp.iso_year2 = Some(read_int(sc, 2, pad)? as u32),
        b'V' => pp.isoweek = Some(read_int(sc, 2, pad)? as u32),
        b'u' => {
            let v = read_int(sc, 1, pad)?;
            pp.weekday = Some(Weekday::from_iso_number(v as u32).ok_or_else(|| {
                Error::invalid("invalid ISO weekday")
            })?);
        }
        b'w' => {
            let v = read_int(sc, 1, pad)?;
            pp.weekday = Some(weekday_from_sunday(v).ok_or_else(|| Error::invalid("invalid weekday"))?);
        }
        b'U' => pp.week_sun = Some(read_int(sc, 2, pad)? as u32),
        b'W' => pp.week_mon = Some(read_int(sc, 2, pad)? as u32),
        b'z' => pp.offset = Some(scan_offset(sc)?),
        b'Z' => {
            let start = sc.pos;
            while sc.peek().is_some_and(|b| b.is_ascii_alphabetic() || b == b'/') {
                sc.pos += 1;
            }
            if sc.pos == start {
                return sc.err("expected a timezone name");
            }
            let word = core::str::from_utf8(&sc.bytes[start..sc.pos])
                .map_err(|_| Error::invalid("invalid utf-8 in input"))?;
            match word {
                "UTC" | "GMT" | "UT" | "Z" | "Etc/UTC" | "Etc/GMT" => {
                    pp.offset = Some(Offset::UTC)
                }
                _ => return Err(Error::invalid("unknown timezone name")),
            }
        }
        b's' => {
            let sign = match sc.peek() {
                Some(b'-') => {
                    sc.pos += 1;
                    -1i128
                }
                Some(b'+') => {
                    sc.pos += 1;
                    1i128
                }
                _ => 1i128,
            };
            let v = read_int(sc, 18, pad)? as i128 * sign;
            if v < i64::MIN as i128 || v > i64::MAX as i128 {
                return sc.err("timestamp out of range");
            }
            pp.timestamp = Some(v as i64);
        }
        b'n' => {
            while matches!(sc.peek(), Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')) {
                sc.pos += 1;
            }
        }
        b't' => {
            while matches!(sc.peek(), Some(b' ') | Some(b'\t')) {
                sc.pos += 1;
            }
        }
        b'%' => {
            if sc.peek() != Some(b'%') {
                return sc.err("expected '%'");
            }
            sc.pos += 1;
        }
        b'F' => {
            parse_directive(sc, pp, b'Y', pad, 0)?;
            if sc.peek() != Some(b'-') {
                return sc.err("expected '-'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'm', pad, 0)?;
            if sc.peek() != Some(b'-') {
                return sc.err("expected '-'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'd', pad, 0)?;
        }
        b'D' | b'x' => {
            parse_directive(sc, pp, b'm', pad, 0)?;
            if sc.peek() != Some(b'/') {
                return sc.err("expected '/'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'd', pad, 0)?;
            if sc.peek() != Some(b'/') {
                return sc.err("expected '/'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'y', pad, 0)?;
        }
        b'R' => {
            parse_directive(sc, pp, b'H', pad, 0)?;
            if sc.peek() != Some(b':') {
                return sc.err("expected ':'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'M', pad, 0)?;
        }
        b'T' | b'X' => {
            parse_directive(sc, pp, b'H', pad, 0)?;
            if sc.peek() != Some(b':') {
                return sc.err("expected ':'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'M', pad, 0)?;
            if sc.peek() != Some(b':') {
                return sc.err("expected ':'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'S', pad, 0)?;
        }
        b'r' => {
            parse_directive(sc, pp, b'I', pad, 0)?;
            if sc.peek() != Some(b':') {
                return sc.err("expected ':'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'M', pad, 0)?;
            if sc.peek() != Some(b':') {
                return sc.err("expected ':'");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'S', pad, 0)?;
            if sc.peek() != Some(b' ') {
                return sc.err("expected ' '");
            }
            sc.pos += 1;
            parse_directive(sc, pp, b'p', pad, 0)?;
        }
        b'+' => {
            // RFC 3339 consumes the rest of the input.
            let rest = core::str::from_utf8(&sc.bytes[sc.pos..])
                .map_err(|_| Error::invalid("invalid utf-8 in input"))?;
            let (date, time, offset) = parse_rfc3339(rest)?;
            pp.year = Some(date.year());
            pp.month = Some(date.month());
            pp.day = Some(date.day());
            let (h, m, s, ns) = time.parts();
            pp.hour = Some(h);
            pp.minute = Some(m);
            pp.second = Some(s);
            pp.nanos = Some(ns);
            pp.offset = Some(offset);
            sc.pos = sc.bytes.len();
        }
        _ => return Err(Error::invalid("unknown format directive")),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-type entry points
// ---------------------------------------------------------------------------

fn parts_from_civil(dt: CivilDateTime, offset: Option<Offset>, zone_name: Option<&'static str>) -> Parts {
    Parts {
        year: dt.year(),
        month: dt.month(),
        day: dt.day(),
        hour: dt.hour(),
        minute: dt.minute(),
        second: dt.second(),
        nanos: dt.nanosecond(),
        offset,
        zone_name,
        timestamp: None,
    }
}

pub(crate) fn format_date(date: Date, fmt: &str) -> Result<String> {
    let p = parts_from_civil(
        CivilDateTime::new(date, TimeOfDay::MIDNIGHT),
        None,
        None,
    );
    format_parts(fmt, &p)
}

pub(crate) fn format_time(time: TimeOfDay, fmt: &str) -> Result<String> {
    let p = parts_from_civil(
        CivilDateTime::new(Date::from_days_since_epoch(0), time),
        None,
        None,
    );
    format_parts(fmt, &p)
}

pub(crate) fn format_civil(dt: CivilDateTime, fmt: &str) -> Result<String> {
    format_parts(fmt, &parts_from_civil(dt, None, None))
}

pub(crate) fn format_ticks(ticks: Ticks, fmt: &str) -> Result<String> {
    let dt = ticks.to_civil_utc()?;
    let mut p = parts_from_civil(dt, Some(Offset::UTC), Some("UTC"));
    // `%s` needs seconds as `i64`; instants beyond that range render it as
    // empty rather than wrapping (the civil projection above already bounds
    // us, but a 9.2e18-day instant can still exceed i64 seconds).
    p.timestamp = i64::try_from(ticks.as_unix_nanos().div_euclid(1_000_000_000)).ok();
    format_parts(fmt, &p)
}

pub(crate) fn format_zoned(zoned: Zoned, fmt: &str) -> Result<String> {
    let dt = zoned.civil()?;
    let zone_name = match zoned.zone() {
        Zone::Utc => Some("UTC"),
        Zone::Fixed(_) => None,
    };
    let mut p = parts_from_civil(dt, Some(zoned.offset()), zone_name);
    p.timestamp = i64::try_from(zoned.ticks().as_unix_nanos().div_euclid(1_000_000_000)).ok();
    format_parts(fmt, &p)
}

pub(crate) fn parse_date(fmt: &str, s: &str) -> Result<Date> {
    parse_parts(fmt, s)?.to_date()
}

pub(crate) fn parse_time(fmt: &str, s: &str) -> Result<TimeOfDay> {
    parse_parts(fmt, s)?.to_time()
}

pub(crate) fn parse_civil(fmt: &str, s: &str) -> Result<CivilDateTime> {
    let pp = parse_parts(fmt, s)?;
    let (date, time) = pp.to_civil()?;
    Ok(CivilDateTime::new(date, time))
}

pub(crate) fn parse_ticks(fmt: &str, s: &str) -> Result<Ticks> {
    parse_parts(fmt, s)?.to_ticks()
}

pub(crate) fn parse_zoned(fmt: &str, s: &str) -> Result<Zoned> {
    let pp = parse_parts(fmt, s)?;
    let offset = pp.offset.unwrap_or(Offset::UTC);
    if let Some(ts) = pp.timestamp {
        return Ok(Zoned::new(Ticks::from_timestamp(ts, 0)?, Zone::fixed(offset)));
    }
    let (date, time) = pp.to_civil()?;
    Zoned::from_civil(CivilDateTime::new(date, time), Zone::fixed(offset))
}

// ---------------------------------------------------------------------------
// RFC 2822 (email / HTTP header dates)
// ---------------------------------------------------------------------------

/// RFC 2822 rendering: `Tue,  1 Jul 2003 10:52:37 +0200`.
pub(crate) fn format_rfc2822(date: Date, time: TimeOfDay, offset: Offset) -> String {
    let mut out = String::new();
    out.push_str(date.weekday().short_name());
    out.push_str(", ");
    pad_num(&mut out, date.day() as u64, 2, b' ');
    out.push(' ');
    out.push_str(Month::from_u32(date.month()).map_or("", |m| m.short_name()));
    out.push(' ');
    emit_signed(&mut out, date.year() as i64, 4, b'0');
    out.push(' ');
    let (h, m, s, _) = time.parts();
    let mut t = String::new();
    pad_num(&mut t, h as u64, 2, b'0');
    out.push_str(&t);
    out.push(':');
    let mut t = String::new();
    pad_num(&mut t, m as u64, 2, b'0');
    out.push_str(&t);
    out.push(':');
    let mut t = String::new();
    pad_num(&mut t, s as u64, 2, b'0');
    out.push_str(&t);
    out.push(' ');
    push_offset(&mut out, offset, false);
    out
}

fn named_zone_offset(name: &str) -> Option<i32> {
    // RFC 2822 section 4.3 fixed named zones.
    let secs = match name {
        "UT" | "GMT" | "UTC" | "Z" => 0,
        "EST" => -5 * 3600,
        "EDT" => -4 * 3600,
        "CST" => -6 * 3600,
        "CDT" => -5 * 3600,
        "MST" => -7 * 3600,
        "MDT" => -6 * 3600,
        "PST" => -8 * 3600,
        "PDT" => -7 * 3600,
        _ => return None,
    };
    Some(secs)
}

/// Parse an RFC 2822 date-time, returning `(date, time, offset)`.
pub(crate) fn parse_rfc2822(s: &str) -> Result<(Date, TimeOfDay, Offset)> {
    let mut sc = Scanner::new(s);

    // Optional leading weekday name (`Mon, ` or `Mon`), verified later.
    let leading_weekday = if sc.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
        parse_weekday_name(&mut sc)?
    } else {
        None
    };
    if sc.peek() == Some(b',') {
        sc.pos += 1;
    }
    skip_spaces(&mut sc);

    // Day (1-2 digits, space padded).
    let day = read_int(&mut sc, 2, b' ')? as u32;
    skip_spaces(&mut sc);
    // Month name.
    let month = parse_month_name(&mut sc)?
        .ok_or_else(|| Error::invalid("expected month name"))?
        .as_u32();
    skip_spaces(&mut sc);
    // Year: 4-digit or obsolete 2-digit.
    let year = {
        let start = sc.pos;
        let v = read_int(&mut sc, 4, b'0')?;
        let digits = sc.pos - start;
        if digits <= 2 {
            // Obsolete: 00-49 -> 2000s, 50-99 -> 1900s.
            if v >= 50 {
                1900 + v as i32
            } else {
                2000 + v as i32
            }
        } else {
            v as i32
        }
    };
    skip_spaces(&mut sc);

    // Time HH:MM[:SS].
    let hour = read_int(&mut sc, 2, b'0')? as u32;
    if sc.peek() != Some(b':') {
        return sc.err("expected ':' after hour");
    }
    sc.pos += 1;
    let minute = read_int(&mut sc, 2, b'0')? as u32;
    let second = if sc.peek() == Some(b':') {
        sc.pos += 1;
        read_int(&mut sc, 2, b'0')? as u32
    } else {
        0
    };
    let time = TimeOfDay::from_hms_nano(hour, minute, second, 0)?;
    let date = Date::from_ymd(year, month, day)?;
    if let Some(wd) = leading_weekday {
        if date.weekday() != wd {
            return Err(Error::invalid("weekday does not match the date"));
        }
    }

    skip_spaces(&mut sc);
    let offset = if sc.at_end() {
        return Err(Error::invalid("missing timezone in RFC 2822 date"));
    } else {
        match sc.peek() {
            Some(b'+') | Some(b'-') => scan_offset(&mut sc)?,
            Some(b'Z') | Some(b'z') => {
                sc.pos += 1;
                Offset::UTC
            }
            _ => {
                let start = sc.pos;
                while sc.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
                    sc.pos += 1;
                }
                let name = core::str::from_utf8(&sc.bytes[start..sc.pos])
                    .map_err(|_| Error::invalid("invalid utf-8 in input"))?;
                let secs = named_zone_offset(name)
                    .ok_or_else(|| Error::invalid("unknown RFC 2822 timezone"))?;
                Offset::from_seconds(secs)?
            }
        }
    };
    if !sc.at_end() {
        return sc.err("trailing characters in input");
    }
    Ok((date, time, offset))
}

fn skip_spaces(sc: &mut Scanner<'_>) {
    while sc.peek() == Some(b' ') {
        sc.pos += 1;
    }
}
