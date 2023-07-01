use std::io;
use std::io::{Cursor, Read};

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::traverse_path::traverse_path;

use super::parse_atom::{parse_atom, parse_path};

const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

#[repr(u8)]
enum ParseOp {
    SExp,
    Cons,
}

/// Deserializes a CLVM node from a `std::io::Cursor`.
pub fn node_from_stream_backrefs(
    allocator: &mut Allocator,
    cursor: &mut Cursor<&[u8]>,
) -> io::Result<NodePtr> {
    let mut values = allocator.null();
    let mut ops = vec![ParseOp::SExp];
    let mut byte = [0; 1];

    loop {
        let op = ops.pop();

        if op.is_none() {
            break;
        }

        match op.unwrap() {
            ParseOp::SExp => {
                cursor.read_exact(&mut byte)?;
                if byte[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if byte[0] == BACK_REFERENCE {
                    let path = parse_path(cursor)?;
                    let reduction = traverse_path(allocator, path, values)?;
                    let back_reference = reduction.1;
                    values = allocator.new_pair(back_reference, values)?;
                } else {
                    let new_atom = parse_atom(allocator, byte[0], cursor)?;
                    values = allocator.new_pair(new_atom, values)?;
                }
            }
            ParseOp::Cons => {
                if let SExp::Pair(v1, v2) = allocator.sexp(values) {
                    if let SExp::Pair(v3, v4) = allocator.sexp(v2) {
                        let new_root = allocator.new_pair(v3, v1)?;
                        values = allocator.new_pair(new_root, v4)?;
                    }
                }
            }
        }
    }

    match allocator.sexp(values) {
        SExp::Pair(value, _) => Ok(value),
        _ => panic!("unexpected atom"),
    }
}

pub fn node_from_bytes_backrefs(allocator: &mut Allocator, bytes: &[u8]) -> io::Result<NodePtr> {
    let mut buffer = Cursor::new(bytes);
    node_from_stream_backrefs(allocator, &mut buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::object_cache::{treehash, ObjectCache};
    use hex::FromHex;

    fn deserialize_check(hex: &str, hash_hex: &str) {
        let bytes = Vec::from_hex(hex).unwrap();
        let mut allocator = Allocator::new();
        let node = node_from_bytes_backrefs(&mut allocator, &bytes).unwrap();

        let mut object_cache = ObjectCache::new(&allocator, treehash);
        let calculated_hash = object_cache.get_or_calculate(&node).unwrap();
        let expected_hash: Vec<u8> = Vec::from_hex(hash_hex).unwrap();
        assert_eq!(expected_hash, calculated_hash);
    }

    #[test]
    fn test_deserialize_with_backrefs() {
        // ("foobar" "foobar")
        deserialize_check(
            "ff86666f6f626172ff86666f6f62617280",
            "9148834131750904c023598bed28db269bdb29012514579e723d63e27829bcba",
        );
        deserialize_check(
            "ff86666f6f626172fe01", // ("foobar" "foobar")
            "9148834131750904c023598bed28db269bdb29012514579e723d63e27829bcba",
        );

        // ((1 2 3 4) 1 2 3 4)
        deserialize_check(
            "ffff01ff02ff03ff0480ff01ff02ff03ff0480",
            "028c16eb4fec600e6153d8dde60eb3916d13d0dc446b5cd7936a1248f8963bf8",
        );
        deserialize_check(
            "ffff01ff02ff03ff0480fe02", // ((1 2 3 4) 1 2 3 4)
            "028c16eb4fec600e6153d8dde60eb3916d13d0dc446b5cd7936a1248f8963bf8",
        );

        // `(((((a_very_long_repeated_string . 1) .  (2 . 3)) . ((4 . 5) .  (6 . 7))) . (8 . 9)) 10 a_very_long_repeated_string)`
        deserialize_check(
            "ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff04\
         05ff0607ff0809ff0aff9b615f766572795f6c6f6e675f72657065617465645f737472696e6780",
            "e23c73777f814e8a4e2785487b272b8b22ddaded1f7cfb808b43f1148602882f",
        );
        deserialize_check(
        "ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff0405ff0607ff0809ff0afffe4180",
        "e23c73777f814e8a4e2785487b272b8b22ddaded1f7cfb808b43f1148602882f",
    );
    }
}
