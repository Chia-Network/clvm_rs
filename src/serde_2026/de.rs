use std::io::{Cursor, Read};

use crate::allocator::{Allocator, NodePtr};
use crate::error::{EvalErr, Result};
use crate::serde::node_from_bytes_backrefs;

use super::MAGIC_PREFIX;
use super::varint::decode_varint;

/// Default maximum atom length (1 MB).
pub const DEFAULT_MAX_ATOM_LEN: usize = 1 << 20;

/// Default maximum input bytes (10 MB).
pub const DEFAULT_MAX_INPUT_BYTES: usize = 10 * (1 << 20);

const MAX_COUNT: usize = 256 * 1024 * 1024;

/// Deserialization limits for the 2026 format.
#[derive(Debug, Clone, Copy)]
pub struct DeserializeLimits {
    /// Maximum byte length of any single atom. Default: 1 MB.
    pub max_atom_len: usize,
    /// Maximum total bytes consumed from the input stream. Default: 10 MB.
    pub max_input_bytes: usize,
}

impl Default for DeserializeLimits {
    fn default() -> Self {
        Self {
            max_atom_len: DEFAULT_MAX_ATOM_LEN,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
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

fn checked_usize(value: i64, max: usize) -> Result<usize> {
    if value < 0 {
        return Err(EvalErr::SerializationError);
    }
    if value as u64 > usize::MAX as u64 {
        return Err(EvalErr::SerializationError);
    }
    let u = value as usize;
    if u > max {
        return Err(EvalErr::SerializationError);
    }
    Ok(u)
}

/// Deserialize CLVM from any format (classic, backrefs, or serde_2026).
///
/// If `bytes` starts with the magic prefix, strip it and deserialize with
/// [`deserialize_2026`]. Otherwise delegate to [`node_from_bytes_backrefs`]
/// (which handles both classic and back-reference formats).
pub fn node_from_bytes_auto(
    allocator: &mut Allocator,
    bytes: &[u8],
    limits: DeserializeLimits,
) -> Result<NodePtr> {
    if bytes.starts_with(&MAGIC_PREFIX) {
        deserialize_2026(allocator, &bytes[MAGIC_PREFIX.len()..], limits)
    } else {
        node_from_bytes_backrefs(allocator, bytes)
    }
}

/// Deserialize a node from a stream using the 2026 format.
///
/// Handles both `cons_lr` (opcode 1) and `cons_rl` (opcode -1), so it can
/// deserialize output from either the default or pair-optimized serializer.
pub fn deserialize_2026_from_stream<R: Read>(
    allocator: &mut Allocator,
    reader: &mut R,
    limits: DeserializeLimits,
) -> Result<NodePtr> {
    let mut reader = LimitReader::new(reader, limits.max_input_bytes);

    let mut atoms: Vec<NodePtr> = Vec::new();
    let group_count = checked_usize(decode_varint(&mut reader)?, MAX_COUNT)?;
    let mut buf: Vec<u8> = Vec::new();

    for _ in 0..group_count {
        let length_val = decode_varint(&mut reader)?;
        let (length, count) = if length_val < 0 {
            if length_val == i64::MIN {
                return Err(EvalErr::SerializationError);
            }
            (
                checked_usize(-length_val, limits.max_atom_len)?,
                checked_usize(decode_varint(&mut reader)?, MAX_COUNT)?,
            )
        } else {
            (checked_usize(length_val, limits.max_atom_len)?, 1)
        };
        buf.resize(length, 0);
        for _ in 0..count {
            reader
                .read_exact(&mut buf)
                .map_err(|_| EvalErr::SerializationError)?;
            atoms.push(allocator.new_atom(&buf)?);
        }
    }

    let instruction_count = checked_usize(decode_varint(&mut reader)?, MAX_COUNT)?;
    if instruction_count == 0 {
        return if atoms.is_empty() {
            Err(EvalErr::SerializationError)
        } else {
            Ok(atoms[0])
        };
    }

    let nil = allocator.nil();
    let mut pairs: Vec<NodePtr> = Vec::with_capacity(instruction_count / 3);
    let mut stack: Vec<NodePtr> = Vec::with_capacity(64);

    for _ in 0..instruction_count {
        let inst = decode_varint(&mut reader)?;
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
    limits: DeserializeLimits,
) -> Result<NodePtr> {
    deserialize_2026_from_stream(allocator, &mut Cursor::new(data), limits)
}

/// Compute the serialized length of a serde_2026 blob (including magic prefix).
///
/// Walks the header structure without allocating or building a CLVM tree.
/// The input buffer may contain trailing data; only the bytes belonging to
/// the serde_2026 blob are counted.
pub fn serialized_length_serde_2026(buf: &[u8]) -> Result<u64> {
    if !buf.starts_with(&MAGIC_PREFIX) {
        return Err(EvalErr::SerializationError);
    }

    let data = &buf[MAGIC_PREFIX.len()..];
    let mut cursor = Cursor::new(data);

    let group_count = checked_usize(decode_varint(&mut cursor)?, MAX_COUNT)?;
    for _ in 0..group_count {
        let length_val = decode_varint(&mut cursor)?;
        let skip = if length_val < 0 {
            if length_val == i64::MIN {
                return Err(EvalErr::SerializationError);
            }
            let atom_len = (-length_val) as u64;
            let count = checked_usize(decode_varint(&mut cursor)?, MAX_COUNT)? as u64;
            atom_len
                .checked_mul(count)
                .ok_or(EvalErr::SerializationError)?
        } else {
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

    let instruction_count = checked_usize(decode_varint(&mut cursor)?, MAX_COUNT)?;
    for _ in 0..instruction_count {
        decode_varint(&mut cursor)?;
    }

    Ok(MAGIC_PREFIX.len() as u64 + cursor.position())
}
