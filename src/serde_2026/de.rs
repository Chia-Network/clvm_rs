use std::io::{Cursor, Read};

use crate::allocator::{Allocator, NodePtr};
use crate::error::{EvalErr, Result};

use super::SERDE_2026_MAGIC_PREFIX;
use super::varint::read_varint;

fn checked_usize(value: i64) -> Result<usize> {
    if value < 0 {
        return Err(EvalErr::SerializationError);
    }
    usize::try_from(value).map_err(|_| EvalErr::SerializationError)
}

fn checked_bounded_usize(value: i64, max: usize) -> Result<usize> {
    let value = checked_usize(value)?;
    if value > max {
        return Err(EvalErr::SerializationError);
    }
    Ok(value)
}

/// Deserialize a node from a stream using the 2026 format.
///
/// **Reads the body only — does *not* expect the magic prefix.** Callers
/// that have a full prefix-framed blob should use [`deserialize_2026`]
/// (slice) or strip the prefix themselves before calling this.
///
/// `max_atom_len` caps the byte length of any single atom (and therefore the
/// pre-allocation done while reading atom bytes). `strict` rejects overlong /
/// non-minimal varint encodings.
///
/// **Caller contract:** `reader` must be bounded — for example via
/// [`std::io::Read::take`] — otherwise a malformed blob can drive an unbounded
/// loop. Total input policy belongs in the caller, not here. Use
/// [`deserialize_2026_body_from_stream`] when you have a slice; the slice's length is the
/// natural bound.
pub fn deserialize_2026_body_from_stream<R: Read>(
    allocator: &mut Allocator,
    reader: &mut R,
    max_atom_len: usize,
    strict: bool,
) -> Result<NodePtr> {
    let mut atoms: Vec<NodePtr> = Vec::new();
    let group_count = checked_usize(read_varint(reader, strict)?)?;
    let mut buf: Vec<u8> = Vec::new();

    for _ in 0..group_count {
        let length_val = read_varint(reader, strict)?;
        let (length, count) = if length_val < 0 {
            if length_val == i64::MIN {
                return Err(EvalErr::SerializationError);
            }
            (
                checked_bounded_usize(-length_val, max_atom_len)?,
                checked_usize(read_varint(reader, strict)?)?,
            )
        } else {
            (checked_bounded_usize(length_val, max_atom_len)?, 1)
        };
        if length == 0 || count == 0 {
            return Err(EvalErr::SerializationError);
        }
        buf.resize(length, 0);
        for _ in 0..count {
            reader
                .read_exact(&mut buf)
                .map_err(|_| EvalErr::SerializationError)?;
            atoms.push(allocator.new_atom(&buf)?);
        }
    }

    let instruction_count = checked_usize(read_varint(reader, strict)?)?;
    if instruction_count == 0 {
        return Err(EvalErr::SerializationError);
    }

    let nil = allocator.nil();
    let mut pairs: Vec<NodePtr> = Vec::new();
    let mut stack: Vec<NodePtr> = Vec::with_capacity(64);

    for _ in 0..instruction_count {
        let inst = read_varint(reader, strict)?;
        match inst {
            0 => stack.push(nil),
            1 => {
                if stack.len() < 2 {
                    return Err(EvalErr::SerializationError);
                }
                let right = stack.pop().unwrap();
                let left = stack.pop().unwrap();
                let pair = allocator.new_pair(left, right)?;
                pairs.push(pair);
                stack.push(pair);
            }
            -1 => {
                if stack.len() < 2 {
                    return Err(EvalErr::SerializationError);
                }
                let left = stack.pop().unwrap();
                let right = stack.pop().unwrap();
                let pair = allocator.new_pair(left, right)?;
                pairs.push(pair);
                stack.push(pair);
            }
            n if n >= 2 => {
                let ai = (n - 2) as usize;
                stack.push(*atoms.get(ai).ok_or(EvalErr::SerializationError)?);
            }
            n => {
                let pi = n
                    .checked_neg()
                    .and_then(|x| x.checked_sub(2))
                    .ok_or(EvalErr::SerializationError)? as usize;
                stack.push(*pairs.get(pi).ok_or(EvalErr::SerializationError)?);
            }
        }
    }

    if stack.len() != 1 {
        return Err(EvalErr::SerializationError);
    }
    Ok(stack[0])
}

/// Deserialize a magic-prefixed serde_2026 blob.
///
/// Verifies and strips [`SERDE_2026_MAGIC_PREFIX`], then delegates to
/// [`deserialize_2026_body_from_stream`]. Pairs with [`super::ser::serialize_2026`].
pub fn deserialize_2026(
    allocator: &mut Allocator,
    blob: &[u8],
    max_atom_len: usize,
    strict: bool,
) -> Result<NodePtr> {
    deserialize_2026_from_stream(allocator, &mut Cursor::new(blob), max_atom_len, strict)
}

/// Deserialize a magic-prefixed serde_2026 blob from a stream.
///
/// Verifies and strips [`SERDE_2026_MAGIC_PREFIX`], then delegates to
/// [`deserialize_2026_body_from_stream`]. Pairs with [`super::ser::serialize_2026_to_stream`].
pub fn deserialize_2026_from_stream<R: Read>(
    allocator: &mut Allocator,
    reader: &mut R,
    max_atom_len: usize,
    strict: bool,
) -> Result<NodePtr> {
    let mut prefix_buf = [0u8; 6];
    reader.read_exact(&mut prefix_buf)?;
    if prefix_buf != SERDE_2026_MAGIC_PREFIX {
        return Err(EvalErr::SerializationError);
    }
    deserialize_2026_body_from_stream(allocator, reader, max_atom_len, strict)
}

/// Compute the serialized length of a serde_2026 blob (including magic prefix).
///
/// Walks the header structure without allocating or building a CLVM tree.
/// Mirrors every header-time validation
/// [`deserialize_2026_body_from_stream`] performs, so a blob that returns
/// `Ok(len)` here is guaranteed to clear the deserializer's header parse
/// (instruction-stream stack invariants and index validity are still checked
/// at deserialize time — those can't be byte-counted).
///
/// `max_atom_len` caps any declared atom length and `strict` rejects
/// overlong / non-minimal varint encodings — pass the same values you would
/// pass to the deserializer. For framing-only callers that don't want a
/// policy opinion, `usize::MAX` and `false` accept anything the deserializer
/// could itself parse on a sufficiently permissive caller.
///
/// The input buffer may contain trailing data; only the bytes belonging to
/// the serde_2026 blob are counted.
pub fn serialized_length_serde_2026(buf: &[u8], max_atom_len: usize, strict: bool) -> Result<u64> {
    if !buf.starts_with(&SERDE_2026_MAGIC_PREFIX) {
        return Err(EvalErr::SerializationError);
    }

    let data = &buf[SERDE_2026_MAGIC_PREFIX.len()..];
    let mut cursor = Cursor::new(data);

    let group_count = checked_usize(read_varint(&mut cursor, strict)?)?;
    for _ in 0..group_count {
        let length_val = read_varint(&mut cursor, strict)?;
        let skip = if length_val < 0 {
            if length_val == i64::MIN {
                return Err(EvalErr::SerializationError);
            }
            let atom_len = checked_bounded_usize(-length_val, max_atom_len)?;
            let count = checked_usize(read_varint(&mut cursor, strict)?)?;
            if atom_len == 0 || count == 0 {
                return Err(EvalErr::SerializationError);
            }
            (atom_len as u64)
                .checked_mul(count as u64)
                .ok_or(EvalErr::SerializationError)?
        } else {
            let atom_len = checked_bounded_usize(length_val, max_atom_len)?;
            if atom_len == 0 {
                return Err(EvalErr::SerializationError);
            }
            atom_len as u64
        };
        let new_pos = cursor
            .position()
            .checked_add(skip)
            .ok_or(EvalErr::SerializationError)?;
        if new_pos > data.len() as u64 {
            return Err(EvalErr::SerializationError);
        }
        cursor.set_position(new_pos);
    }

    let instruction_count = checked_usize(read_varint(&mut cursor, strict)?)?;
    // Mirror `deserialize_2026_body_from_stream`: instruction_count == 0
    // leaves the stack empty and is rejected there, so reject it here too.
    if instruction_count == 0 {
        return Err(EvalErr::SerializationError);
    }
    for _ in 0..instruction_count {
        read_varint(&mut cursor, strict)?;
    }

    Ok(SERDE_2026_MAGIC_PREFIX.len() as u64 + cursor.position())
}
