//! `rustbinary` compact binary facade.
//!
//! The binary profile is not a separate codec — it is the same
//! `NsonSerialize` / `NsonDeserialize` implementations selected by the
//! codec's `is_human_readable() == false`. These helpers just shorten the
//! call path to `rustbinary`'s bounded, self-describing wire format.
//!
//! ```
//! # use tzcraft::{Date, Ticks};
//! let date = Date::from_ymd(2024, 6, 15).unwrap();
//! let bytes = tzcraft::binary::encode(&date).unwrap();
//! let back: Date = tzcraft::binary::decode(&bytes).unwrap();
//! assert_eq!(date, back);
//! ```
//!
//! Untrusted input should be decoded through a tuned [`Config`] (see
//! [`options`]) with explicit size limits, exactly as with any other
//! `rustbinary` consumer.

use alloc::vec::Vec;

pub use rustbinary::{options, Config, Error as BinaryError, Result as BinaryResult};

use nextjson::{NsonDeserialize, NsonSerialize};

/// Encode a value into a `Vec<u8>` with the standard compact profile.
pub fn encode<T: NsonSerialize + ?Sized>(value: &T) -> BinaryResult<Vec<u8>> {
    rustbinary::serialize(value)
}

/// Decode a value from a byte slice with the standard compact profile.
///
/// Borrowed targets may point into `input`.
pub fn decode<'de, T: NsonDeserialize<'de>>(input: &'de [u8]) -> BinaryResult<T> {
    rustbinary::deserialize(input)
}

/// Encode into a caller-owned slice without codec-owned allocation.
///
/// Returns the number of bytes written.
pub fn encode_into_slice<T: NsonSerialize + ?Sized>(
    output: &mut [u8],
    value: &T,
) -> BinaryResult<usize> {
    rustbinary::serialize_into_slice(output, value)
}

/// Exact serialized byte count without allocating output.
pub fn encoded_size<T: NsonSerialize + ?Sized>(value: &T) -> BinaryResult<u64> {
    rustbinary::serialized_size(value)
}
