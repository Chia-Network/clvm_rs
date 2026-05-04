use std::io::{Cursor, Read};

use crate::allocator::{Allocator, NodePtr};
use crate::error::{EvalErr, Result};
use crate::serde::node_from_bytes_backrefs;

use super::SERDE_2026_MAGIC_PREFIX;
use super::varint::decode_varint;

/// Default maximum atom length (1 MB).
pub const DEFAULT_MAX_ATOM_LEN: usize = 1 << 20;

/// Default maximum input bytes (10 MB).
pub const DEFAULT_MAX_INPUT_BYTES: usize = 10 * (1 << 20);

/// Deserialization options for the 2026 format.
#[derive(Debug, Clone, Copy)]
pub struct DeserializeOptions {
    /// Maximum byte length of any single atom. Default: 1 MB.
    pub max_atom_len: usize,
    /// Maximum total bytes consumed from the input stream. Default: 10 MB.
    ///
    /// This also bounds atom groups, atoms, instructions, stack growth, and pair
    /// allocation: every declared item must consume at least one byte before it
    /// can produce work.
    pub max_input_bytes: usize,
    /// If true, reject overlong/non-minimal varint encodings.
    pub strict: bool,
}

impl Default for DeserializeOptions {
    fn default() -> Self {
        Self {
            max_atom_len: DEFAULT_MAX_ATOM_LEN,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            strict: false,
        }
    }
}

/// Wraps a `Read` and enforces a byte budget.
struct LimitReader<R> {
    inner: R,
    remaining: usize,
}

impl<R: Read> LimitReader<R> {
    fn new(inner: R, max_bytes: usize) -> Self {
        Self {
            inner,
            remaining: max_bytes,
        }
    }
}

impl<R: Read> Read for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let limit = buf.len().min(self.remaining);
        if limit == 0 && !buf.is_empty() {
            return Err(std::io::Error::other(
                "serde_2026: input exceeds max_input_bytes",
            ));
        }
        let n = self.inner.read(&mut buf[..limit])?;
        self.remaining -= n;
        Ok(n)
    }
}

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

/// Deserialize CLVM from any format (classic, backrefs, or serde_2026).
///
/// If `bytes` starts with the magic prefix, strip it and deserialize with
/// [`deserialize_2026`]. Otherwise delegate to [`node_from_bytes_backrefs`]
/// (which handles both classic and back-reference formats).
pub fn node_from_bytes_auto(
    allocator: &mut Allocator,
    bytes: &[u8],
    options: DeserializeOptions,
) -> Result<NodePtr> {
    if bytes.starts_with(&SERDE_2026_MAGIC_PREFIX) {
        deserialize_2026(allocator, &bytes[SERDE_2026_MAGIC_PREFIX.len()..], options)
    } else {
        node_from_bytes_backrefs(allocator, bytes)
    }
}

/// Deserialize a node from a stream using the 2026 format.
pub fn deserialize_2026_from_stream<R: Read>(
    allocator: &mut Allocator,
    reader: &mut R,
    options: DeserializeOptions,
) -> Result<NodePtr> {
    let mut reader = LimitReader::new(reader, options.max_input_bytes);

    let mut atoms: Vec<NodePtr> = Vec::new();
    let group_count = checked_usize(decode_varint(&mut reader, options.strict)?)?;
    let mut buf: Vec<u8> = Vec::new();

    for _ in 0..group_count {
        let length_val = decode_varint(&mut reader, options.strict)?;
        let (length, count) = if length_val < 0 {
            if length_val == i64::MIN {
                return Err(EvalErr::SerializationError);
            }
            (
                checked_bounded_usize(-length_val, options.max_atom_len)?,
                checked_usize(decode_varint(&mut reader, options.strict)?)?,
            )
        } else {
            (checked_bounded_usize(length_val, options.max_atom_len)?, 1)
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

    let instruction_count = checked_usize(decode_varint(&mut reader, options.strict)?)?;
    if instruction_count == 0 {
        return Err(EvalErr::SerializationError);
    }

    let nil = allocator.nil();
    // Don't pre-size `pairs` from `instruction_count`: a tiny blob could
    // declare ~2^54 instructions and trick `Vec::with_capacity` into a
    // multi-PB allocation request that aborts the process. `LimitReader`
    // already bounds total bytes consumed, so the loop below cannot iterate
    // far enough to drive `pairs` past `max_input_bytes`/3 entries — geometric
    // growth from `Vec::new()` handles that safely with a handful of reallocs.
    let mut pairs: Vec<NodePtr> = Vec::new();
    let mut stack: Vec<NodePtr> = Vec::with_capacity(64);

    for _ in 0..instruction_count {
        let inst = decode_varint(&mut reader, options.strict)?;
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
                let pi = (-n - 2) as usize;
                stack.push(*pairs.get(pi).ok_or(EvalErr::SerializationError)?);
            }
        }
    }

    if stack.len() != 1 {
        return Err(EvalErr::SerializationError);
    }
    Ok(stack[0])
}

/// Deserialize a node from bytes using the 2026 format.
pub fn deserialize_2026(
    allocator: &mut Allocator,
    data: &[u8],
    options: DeserializeOptions,
) -> Result<NodePtr> {
    deserialize_2026_from_stream(allocator, &mut Cursor::new(data), options)
}

/// Compute the serialized length of a serde_2026 blob (including magic prefix).
///
/// Walks the header structure without allocating or building a CLVM tree.
/// The input buffer may contain trailing data; only the bytes belonging to
/// the serde_2026 blob are counted.
pub fn serialized_length_serde_2026(buf: &[u8]) -> Result<u64> {
    if !buf.starts_with(&SERDE_2026_MAGIC_PREFIX) {
        return Err(EvalErr::SerializationError);
    }

    let data = &buf[SERDE_2026_MAGIC_PREFIX.len()..];
    let mut cursor = Cursor::new(data);

    let group_count = checked_usize(decode_varint(&mut cursor, false)?)?;
    for _ in 0..group_count {
        let length_val = decode_varint(&mut cursor, false)?;
        let skip = if length_val < 0 {
            if length_val == i64::MIN {
                return Err(EvalErr::SerializationError);
            }
            let atom_len = (-length_val) as u64;
            let count = checked_usize(decode_varint(&mut cursor, false)?)?;
            if atom_len == 0 || count == 0 {
                return Err(EvalErr::SerializationError);
            }
            let count = count as u64;
            atom_len
                .checked_mul(count)
                .ok_or(EvalErr::SerializationError)?
        } else {
            if length_val == 0 {
                return Err(EvalErr::SerializationError);
            }
            length_val as u64
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

    let instruction_count = checked_usize(decode_varint(&mut cursor, false)?)?;
    for _ in 0..instruction_count {
        decode_varint(&mut cursor, false)?;
    }

    Ok(SERDE_2026_MAGIC_PREFIX.len() as u64 + cursor.position())
}
