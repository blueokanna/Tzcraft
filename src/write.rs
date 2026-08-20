//! Allocation-free output sinks.
//!
//! Everything in this module works under `#![no_std]` **without** an
//! allocator. The crate never calls `alloc` to format a value: it writes
//! into a caller-owned byte slice through [`Write`](crate::write::Write) and
//! [`Buf`](crate::write::Buf). The
//! `String`-returning convenience methods (`to_rfc3339`, `format`, ...) are
//! thin wrappers over the same machinery and are gated behind the `alloc`
//! feature.
//!
//! [`core::fmt::Write`] is not available on every `no_std` target this crate
//! supports, so formatting uses this small three-method trait instead. The
//! set of implementors shipped here is closed ([`Buf`](crate::write::Buf) for stack buffers and
//! an internal adapter for `fmt::Display`), which keeps the trait object
//! safe and the dispatch cost negligible.

use core::fmt;

use crate::error::{Error, Result};

#[cfg(feature = "alloc")]
use alloc::string::String;

/// A minimal byte-oriented output sink.
///
/// The required method is [`Write::write_bytes`]; `write_str`, `write_byte`
/// and `write_char` have default implementations on top of it. Text sinks
/// such as [`Buf`] reject byte slices that are not independently valid UTF-8.
pub trait Write {
    /// Append a UTF-8 byte slice. Implementations used as text sinks return
    /// an error for invalid UTF-8.
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()>;

    /// Append a `&str` (its UTF-8 encoding is written verbatim).
    #[inline]
    fn write_str(&mut self, s: &str) -> Result<()> {
        self.write_bytes(s.as_bytes())
    }

    /// Append one byte.
    #[inline]
    fn write_byte(&mut self, b: u8) -> Result<()> {
        self.write_bytes(core::slice::from_ref(&b))
    }

    /// Append one character, UTF-8 encoded.
    #[inline]
    fn write_char(&mut self, c: char) -> Result<()> {
        let mut buf = [0u8; 4];
        self.write_bytes(c.encode_utf8(&mut buf).as_bytes())
    }
}

impl Write for &mut dyn Write {
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        (**self).write_bytes(bytes)
    }
}

/// A fixed-capacity buffer over caller-owned bytes: the allocator-free way
/// to format a value.
///
/// ```
/// use tzcraft::write::Buf;
/// use tzcraft::write::Write;
///
/// let mut storage = [0u8; 64];
/// let mut buf = Buf::new(&mut storage);
/// buf.write_str("hello")?;
/// assert_eq!(buf.as_str(), "hello");
/// # Ok::<(), tzcraft::Error>(())
/// ```
#[derive(Debug)]
pub struct Buf<'a> {
    data: &'a mut [u8],
    len: usize,
}

impl<'a> Buf<'a> {
    /// Wrap `data` as an empty sink; content is appended from offset 0.
    pub fn new(data: &'a mut [u8]) -> Buf<'a> {
        Buf { data, len: 0 }
    }

    /// Number of bytes written so far.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written yet.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Total capacity of the backing slice.
    #[inline]
    pub const fn capacity(&self) -> usize {
        self.data.len()
    }

    /// The written bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..self.len]
    }

    /// The written bytes as a `&str`.
    ///
    /// This never fails: every byte written through [`Write`] is valid UTF-8
    /// by construction (the formatter only emits ASCII digits, ASCII
    /// punctuation and literal UTF-8 text from format strings).
    #[inline]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.as_bytes()).expect("Buf only ever holds valid UTF-8")
    }

    /// Reset to empty, reusing the backing slice.
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl Write for Buf<'_> {
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        core::str::from_utf8(bytes).map_err(|_| Error::invalid("invalid utf-8"))?;
        let end = self
            .len
            .checked_add(bytes.len())
            .ok_or_else(Error::buffer_overflow)?;
        if end > self.data.len() {
            return Err(Error::buffer_overflow());
        }
        self.data[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }
}

/// The `alloc`-feature formatting wrappers write into a fresh `String`;
/// that sink can always grow, so writes never fail.
#[cfg(feature = "alloc")]
impl Write for alloc::string::String {
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let s = core::str::from_utf8(bytes).map_err(|_| Error::invalid("invalid utf-8"))?;
        self.push_str(s);
        Ok(())
    }
}

/// Adapter that routes a [`Write`](crate::write::Write) into a
/// `core::fmt::Formatter`, used by
/// the crate's `fmt::Display` impls so they never allocate.
pub(crate) struct FmtSink<'a, 'b>(pub(crate) &'a mut fmt::Formatter<'b>);

impl Write for FmtSink<'_, '_> {
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let s = core::str::from_utf8(bytes).map_err(|_| Error::invalid("invalid utf-8"))?;
        self.0.write_str(s).map_err(|_| Error::invalid("fmt error"))
    }
}

/// Run `f` over a [`Buf`](crate::write::Buf) wrapping `out` and return the
/// number of bytes
/// written. Shared by every `write_*` public method.
#[inline]
pub(crate) fn with_buf(
    out: &mut [u8],
    f: impl FnOnce(&mut Buf<'_>) -> Result<()>,
) -> Result<usize> {
    let mut buf = Buf::new(out);
    f(&mut buf)?;
    Ok(buf.len())
}

/// Format into a fixed 64-byte stack buffer and convert to a `String` with
/// **one** allocation.
///
/// The fixed-length formatters (RFC 3339 / RFC 2822 / ISO 8601 date, time
/// and offset) never produce more than 64 bytes, so this avoids both the
/// repeated `String` growth of direct writing and the per-byte virtual
/// dispatch of writing to `&mut String` directly. A single final copy is
/// all that reaches the heap.
#[cfg(feature = "alloc")]
pub(crate) fn alloc_string<F: FnOnce(&mut Buf<'_>) -> Result<()>>(f: F) -> Result<String> {
    let mut storage = [0u8; 64];
    let mut buf = Buf::new(&mut storage);
    f(&mut buf)?;
    let s = core::str::from_utf8(buf.as_bytes()).expect("formatter output is ASCII");
    Ok(alloc::string::String::from(s))
}

/// Emit `value` right-justified to at least `width` digits with zero padding.
///
/// `value` must be non-negative. Up to 20 digits are supported, which covers
/// `i64::MAX`; the caller is responsible for larger ranges (see
/// [`write_u128`]).
#[inline]
pub(crate) fn write_padded(out: &mut dyn Write, value: i64, width: usize) -> Result<()> {
    debug_assert!(value >= 0);
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    let mut n = value;
    loop {
        digits[len] = (n % 10) as u8;
        n /= 10;
        len += 1;
        if n == 0 {
            break;
        }
    }
    for _ in len..width {
        out.write_byte(b'0')?;
    }
    for i in (0..len).rev() {
        out.write_byte(b'0' + digits[i])?;
    }
    Ok(())
}

/// Emit an unsigned 128-bit value with no padding (used by ISO 8601 duration
/// rendering, where day/hour/minute counts can exceed 64 bits).
#[inline]
pub(crate) fn write_u128(out: &mut dyn Write, mut value: u128) -> Result<()> {
    let mut digits = [0u8; 39];
    let mut len = 0usize;
    loop {
        digits[len] = (value % 10) as u8;
        value /= 10;
        len += 1;
        if value == 0 {
            break;
        }
    }
    for i in (0..len).rev() {
        out.write_byte(b'0' + digits[i])?;
    }
    Ok(())
}

/// Emit a signed 128-bit value with no padding.
///
/// Unlike `value as u128` (which wraps negatives modulo 2^128 into a huge
/// positive), this emits a leading `-` for negative values, mirroring what
/// `alloc::format!("{}", value)` would produce. Used by the extreme-instant
/// fallback paths of `Ticks::write_rfc3339` / `Ticks::write_rfc2822` so they
/// stay consistent with their allocating counterparts.
#[inline]
pub(crate) fn write_signed_i128(out: &mut dyn Write, value: i128) -> Result<()> {
    if value < 0 {
        out.write_byte(b'-')?;
        write_u128(out, value.unsigned_abs())
    } else {
        write_u128(out, value as u128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buf_writes_and_reports_capacity() {
        let mut storage = [0u8; 8];
        let mut buf = Buf::new(&mut storage);
        assert!(buf.is_empty());
        assert_eq!(buf.capacity(), 8);
        buf.write_str("abc").unwrap();
        buf.write_byte(b'd').unwrap();
        buf.write_char('é').unwrap();
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_str(), "abcdé");
        // Overflow is an error, not a panic or a silent truncation.
        assert!(buf.write_str("xyz").is_err());
        assert_eq!(buf.as_str(), "abcdé");
        assert!(buf.write_bytes(&[0xff]).is_err());
        assert_eq!(buf.as_str(), "abcdé");
        buf.clear();
        assert!(buf.is_empty());
    }

    #[test]
    fn padded_and_u128() {
        let mut storage = [0u8; 64];
        let mut buf = Buf::new(&mut storage);
        write_padded(&mut buf, 42, 4).unwrap();
        write_u128(&mut buf, 0).unwrap();
        write_u128(&mut buf, u128::MAX).unwrap();
        // u128::MAX = 340282366920938463463374607431768211455.
        assert_eq!(buf.as_str(), "00420340282366920938463463374607431768211455");
    }
}
