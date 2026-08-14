//! Codec integration: one implementation per type, two wire shapes.
//!
//! Every `tzcraft` type implements `nextjson`'s format-neutral contracts
//! (`NsonSchema` + `NsonSerialize` + `NsonDeserialize`) exactly once. The
//! codec selects the wire shape through
//! [`FormatEncoder::is_human_readable`] / [`FormatDecoder::is_human_readable`]:
//!
//! | type         | human-readable (nextjson JSON)      | binary (rustbinary)          |
//! |--------------|-------------------------------------|------------------------------|
//! | `Ticks`      | RFC 3339 string                     | `i128` nanoseconds           |
//! | `Duration`   | ISO 8601 duration string            | `i128` nanoseconds           |
//! | `Date`       | `YYYY-MM-DD` string                 | `i32` days                   |
//! | `TimeOfDay`  | `HH:MM:SS[.f]` string               | `u64` nanoseconds-of-day     |
//! | `CivilDateTime` | `YYYY-MM-DDTHH:MM:SS[.f]` string | packed `i128`                |
//! | `Offset`     | `+08:00` / `Z` string               | `i32` seconds                |
//! | `Zone`       | `UTC` / offset string               | tagged array                 |
//! | `Zoned`      | RFC 3339 string with offset         | `[ticks, offset]` array      |
//! | `Weekday`    | `"Monday"` / number                 | `u8` discriminant            |
//! | `Month`      | `"January"` / number                | `u8` month number            |
//!
//! JSON therefore stays human-readable and self-describing while the binary
//! profile stays compact — with no separate serde module and no feature that
//! toggles the shape.

use alloc::string::ToString;

// `Token` is a private helper; the format-neutral contracts are re-exported
// at the bottom of this module (they are part of the public surface).
use nextjson::Token;

use crate::calendar::{Month, Weekday, NS_PER_DAY};
use crate::date::Date;
use crate::datetime::CivilDateTime;
use crate::duration::Duration;
use crate::format::FractionDigits;
use crate::offset::Offset;
use crate::ticks::Ticks;
use crate::time::TimeOfDay;
use crate::zone::Zone;
use crate::zoned::Zoned;

/// All `tzcraft` types describe their canonical (human) form as a string.
macro_rules! impl_schema_str {
    ($($t:ty),* $(,)?) => {$(
        impl NsonSchema for $t {
            const SCHEMA: TypeSchema = TypeSchema::Str;
        }
    )*};
}
impl_schema_str!(
    Ticks,
    Duration,
    Date,
    TimeOfDay,
    CivilDateTime,
    Offset,
    Zone,
    Zoned,
    Weekday,
    Month,
);

/// Map a `tzcraft` error into the codec's error type.
fn codec_err<E: nextjson::FormatError>(e: crate::error::Error) -> E {
    E::custom(e.to_string())
}

// ---------------------------------------------------------------------------
// Ticks
// ---------------------------------------------------------------------------

impl NsonSerialize for Ticks {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_rfc3339(FractionDigits::Auto))
        } else {
            enc.write_i128(self.as_unix_nanos())
        }
    }
}

impl<'de> NsonDeserialize<'de> for Ticks {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = Ticks::from_rfc3339(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            out.write(Ticks::from_unix_nanos(dec.i128()?));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Duration
// ---------------------------------------------------------------------------

impl NsonSerialize for Duration {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_iso8601())
        } else {
            enc.write_i128(self.as_nanos())
        }
    }
}

impl<'de> NsonDeserialize<'de> for Duration {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = Duration::from_iso8601(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            out.write(Duration::from_nanos(dec.i128()?));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Date
// ---------------------------------------------------------------------------

impl NsonSerialize for Date {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_iso())
        } else {
            enc.write_i32(self.days_since_epoch())
        }
    }
}

impl<'de> NsonDeserialize<'de> for Date {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = Date::from_iso(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            out.write(Date::from_days_since_epoch(dec.i32()?));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TimeOfDay
// ---------------------------------------------------------------------------

impl NsonSerialize for TimeOfDay {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_iso())
        } else {
            enc.write_u64(self.nanos_since_midnight())
        }
    }
}

impl<'de> NsonDeserialize<'de> for TimeOfDay {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = TimeOfDay::from_iso(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            let v = TimeOfDay::from_nanos_since_midnight(dec.u64()?).map_err(codec_err)?;
            out.write(v);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CivilDateTime
// ---------------------------------------------------------------------------

impl NsonSerialize for CivilDateTime {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_iso())
        } else {
            let packed = self.date().days_since_epoch() as i128 * NS_PER_DAY
                + self.time().nanos_since_midnight() as i128;
            enc.write_i128(packed)
        }
    }
}

impl<'de> NsonDeserialize<'de> for CivilDateTime {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = CivilDateTime::from_iso(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            let packed = dec.i128()?;
            let days = i64::try_from(packed.div_euclid(NS_PER_DAY))
                .map_err(|_| D::Error::custom("civil date-time out of range"))?;
            let date = Date::from_days_checked(days).map_err(codec_err)?;
            let time = TimeOfDay::from_nanos_since_midnight(packed.rem_euclid(NS_PER_DAY) as u64)
                .map_err(codec_err)?;
            out.write(CivilDateTime::new(date, time));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Offset
// ---------------------------------------------------------------------------

impl NsonSerialize for Offset {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_iso())
        } else {
            enc.write_i32(self.as_seconds())
        }
    }
}

impl<'de> NsonDeserialize<'de> for Offset {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = Offset::from_iso(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            let v = Offset::from_seconds(dec.i32()?).map_err(codec_err)?;
            out.write(v);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Zone
// ---------------------------------------------------------------------------

impl NsonSerialize for Zone {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_iso())
        } else {
            enc.begin_array()?;
            enc.separator()?;
            match *self {
                Zone::Utc => enc.write_u8(0)?,
                Zone::Fixed(offset) => {
                    enc.write_u8(1)?;
                    enc.separator()?;
                    enc.write_i32(offset.as_seconds())?;
                }
            }
            enc.end_array()
        }
    }
}

impl<'de> NsonDeserialize<'de> for Zone {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = Zone::from_iso(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            dec.begin_array()?;
            let tag = dec.u8()?;
            let mut payload: Option<i32> = None;
            let mut count = 1u32;
            while dec.array_has_more()? {
                payload = Some(dec.i32()?);
                count += 1;
                if !dec.array_entry_sep()? {
                    break;
                }
            }
            dec.end_array()?;
            let zone = match (tag, payload, count) {
                (0, None, 1) => Zone::Utc,
                (1, Some(secs), 2) => {
                    let offset = Offset::from_seconds(secs).map_err(codec_err)?;
                    Zone::fixed(offset)
                }
                _ => return Err(D::Error::custom("invalid zone encoding")),
            };
            out.write(zone);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Zoned
// ---------------------------------------------------------------------------

impl NsonSerialize for Zoned {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(&self.to_rfc3339(FractionDigits::Auto))
        } else {
            enc.begin_array()?;
            enc.separator()?;
            enc.write_i128(self.ticks().as_unix_nanos())?;
            enc.separator()?;
            enc.write_i32(self.zone().offset().as_seconds())?;
            enc.end_array()
        }
    }
}

impl<'de> NsonDeserialize<'de> for Zoned {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let s = dec.string()?;
            let v = Zoned::from_rfc3339(&s).map_err(codec_err)?;
            out.write(v);
        } else {
            dec.begin_array()?;
            let ticks = Ticks::from_unix_nanos(dec.i128()?);
            let offset = if dec.array_has_more()? {
                let seconds = dec.i32()?;
                if dec.array_entry_sep()? {
                    return Err(D::Error::custom("trailing data in zoned encoding"));
                }
                seconds
            } else {
                return Err(D::Error::custom("missing offset in zoned encoding"));
            };
            dec.end_array()?;
            let offset = Offset::from_seconds(offset).map_err(codec_err)?;
            out.write(Zoned::new(ticks, Zone::fixed(offset)));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Weekday / Month
// ---------------------------------------------------------------------------

impl NsonSerialize for Weekday {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(self.name())
        } else {
            enc.write_u8(self.as_u8())
        }
    }
}

impl<'de> NsonDeserialize<'de> for Weekday {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let v = match dec.peek_token()? {
                Token::Number(_) => {
                    let n = dec.u8()?;
                    Weekday::from_discriminant(n)
                        .ok_or_else(|| D::Error::custom("invalid weekday number"))?
                }
                _ => {
                    let s = dec.string()?;
                    Weekday::from_name(&s)
                        .ok_or_else(|| D::Error::custom("unknown weekday name"))?
                }
            };
            out.write(v);
        } else {
            let v = Weekday::from_discriminant(dec.u8()?)
                .ok_or_else(|| D::Error::custom("invalid weekday discriminant"))?;
            out.write(v);
        }
        Ok(())
    }
}

impl NsonSerialize for Month {
    fn nextencode<E: FormatEncoder>(&self, enc: &mut E) -> Result<(), E::Error> {
        if enc.is_human_readable() {
            enc.write_str(self.name())
        } else {
            enc.write_u8(self.as_u32() as u8)
        }
    }
}

impl<'de> NsonDeserialize<'de> for Month {
    fn nextdecode_into<D: FormatDecoder<'de>>(
        dec: &mut D,
        out: &mut DecodeSlot<Self>,
    ) -> Result<(), D::Error> {
        if dec.is_human_readable() {
            let v = match dec.peek_token()? {
                Token::Number(_) => {
                    let n = dec.u8()?;
                    Month::from_u32(n as u32)
                        .ok_or_else(|| D::Error::custom("invalid month number"))?
                }
                _ => {
                    let s = dec.string()?;
                    Month::from_name(&s).ok_or_else(|| D::Error::custom("unknown month name"))?
                }
            };
            out.write(v);
        } else {
            let v = Month::from_u32(dec.u8()? as u32)
                .ok_or_else(|| D::Error::custom("invalid month"))?;
            out.write(v);
        }
        Ok(())
    }
}

/// Re-export of the `nextjson` format-neutral contracts used by `tzcraft`.
///
/// Available as `tzcraft::codec::NsonSerialize` and friends so downstream
/// code can write `tzcraft::codec::nextencode(...)` without naming `nextjson`
/// directly.
pub use nextjson::{
    DecodeSlot, FormatDecoder, FormatEncoder, FormatError, NsonDeserialize, NsonSchema,
    NsonSerialize, TypeSchema,
};
