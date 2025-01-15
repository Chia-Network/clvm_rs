use std::collections::HashSet;
use std::io;
use std::io::{Cursor, Read};

use super::parse_atom::{parse_atom, parse_path};
use crate::allocator::{Allocator, NodePtr, SExp};
use crate::reduction::EvalErr;
use crate::traverse_path::{first_non_zero, msb_mask, traverse_path};

const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

#[repr(u8)]
enum ParseOp {
    SExp,
    Cons,
}

/// deserialize a clvm node from a `std::io::Cursor`
pub fn node_from_stream_backrefs(
    allocator: &mut Allocator,
    f: &mut Cursor<&[u8]>,
    mut backref_callback: impl FnMut(NodePtr),
) -> io::Result<NodePtr> {
    let mut values = Vec::<NodePtr>::new();
    let mut ops = vec![ParseOp::SExp];

    let mut b = [0; 1];
    while let Some(op) = ops.pop() {
        match op {
            ParseOp::SExp => {
                f.read_exact(&mut b)?;
                if b[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if b[0] == BACK_REFERENCE {
                    let path = parse_path(f)?;
                    let back_reference = traverse_path_with_vec(allocator, path, &values)?;
                    backref_callback(back_reference);
                    values.push(back_reference);
                } else {
                    let new_atom = parse_atom(allocator, b[0], f)?;
                    values.push(new_atom);
                }
            }
            ParseOp::Cons => {
                // cons
                // pop left and right values off of the "values" stack, then
                // push the new pair onto it
                let right = values.pop().expect("No cons without two vals.");
                let left = values.pop().expect("No cons without two vals.");
                let root_node = allocator.new_pair(left, right)?;
                values.push(root_node);
            }
        }
    }
    Ok(values.pop().expect("Top of the stack"))
}

fn node_from_stream_backrefs_old(
    allocator: &mut Allocator,
    f: &mut Cursor<&[u8]>,
    mut backref_callback: impl FnMut(NodePtr),
) -> io::Result<NodePtr> {
    let mut values = allocator.nil();
    let mut ops = vec![ParseOp::SExp];

    let mut b = [0; 1];
    while let Some(op) = ops.pop() {
        match op {
            ParseOp::SExp => {
                f.read_exact(&mut b)?;
                if b[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if b[0] == BACK_REFERENCE {
                    let path = parse_path(f)?;
                    let reduction = traverse_path(allocator, path, values)?;
                    let back_reference = reduction.1;
                    backref_callback(back_reference);
                    values = allocator.new_pair(back_reference, values)?;
                } else {
                    let new_atom = parse_atom(allocator, b[0], f)?;
                    values = allocator.new_pair(new_atom, values)?;
                }
            }
            ParseOp::Cons => {
                // cons
                // pop left and right values off of the "values" stack, then
                // push the new pair onto it
                let SExp::Pair(right, rest) = allocator.sexp(values) else {
                    panic!("internal error");
                };
                let SExp::Pair(left, rest) = allocator.sexp(rest) else {
                    panic!("internal error");
                };
                let new_root = allocator.new_pair(left, right)?;
                values = allocator.new_pair(new_root, rest)?;
            }
        }
    }
    match allocator.sexp(values) {
        SExp::Pair(v1, _v2) => Ok(v1),
        _ => panic!("unexpected atom"),
    }
}

pub fn node_from_bytes_backrefs(allocator: &mut Allocator, b: &[u8]) -> io::Result<NodePtr> {
    let mut buffer = Cursor::new(b);
    node_from_stream_backrefs(allocator, &mut buffer, |_node| {})
}

pub fn node_from_bytes_backrefs_old(allocator: &mut Allocator, b: &[u8]) -> io::Result<NodePtr> {
    let mut buffer = Cursor::new(b);
    node_from_stream_backrefs_old(allocator, &mut buffer, |_node| {})
}

pub fn node_from_bytes_backrefs_record(
    allocator: &mut Allocator,
    b: &[u8],
) -> io::Result<(NodePtr, HashSet<NodePtr>)> {
    let mut buffer = Cursor::new(b);
    let mut backrefs = HashSet::<NodePtr>::new();
    let ret = node_from_stream_backrefs(allocator, &mut buffer, |node| {
        backrefs.insert(node);
    })?;
    Ok((ret, backrefs))
}

pub fn traverse_path_with_vec(
    allocator: &mut Allocator,
    node_index: &[u8],
    args: &[NodePtr],
) -> io::Result<NodePtr> {
    // the vec is a stack so a ChiaLisp list of (3 . (2 . (1 . NIL))) would be [1, 2, 3]
    // however entries in this vec may be ChiaLisp SExps so it may look more like [1, (2 . NIL), 3]

    let mut parsing_sexp = false;
    if args.is_empty() {
        parsing_sexp = true;
    }

    // instead of popping, we treat this as a pointer to the end of the virtual stack
    let mut arg_index: usize = if parsing_sexp { 0 } else { args.len() - 1 };

    // find first non-zero byte
    let first_bit_byte_index = first_non_zero(node_index);
    if first_bit_byte_index >= node_index.len() {
        return Ok(NodePtr::NIL);
    }

    // find first non-zero bit (the most significant bit is a sentinel)
    let last_bitmask = msb_mask(node_index[first_bit_byte_index]);

    // follow through the bits, moving left and right
    let mut byte_idx = node_index.len() - 1;
    let mut bitmask = 0x01;

    // if we move from parsing the Vec stack to parsing the SExp stack use the following variables
    let mut sexp_to_parse = NodePtr::NIL;

    while byte_idx > first_bit_byte_index || bitmask < last_bitmask {
        let is_bit_set: bool = (node_index[byte_idx] & bitmask) != 0;
        if parsing_sexp {
            match allocator.sexp(sexp_to_parse) {
                SExp::Atom => {
                    return Err(EvalErr(sexp_to_parse, "path into atom".into()).into());
                }
                SExp::Pair(left, right) => {
                    sexp_to_parse = if is_bit_set { right } else { left };
                }
            }
        } else if is_bit_set {
            // we have traversed right ("rest"), so we keep processing the Vec
            // pop from the stack
            if arg_index == 0 {
                return Err(EvalErr(sexp_to_parse, "reference not in stack".into()).into());
            }
            arg_index -= 1;
        } else {
            // we have traversed left (i.e "first" rather than "rest") so we must process as SExp now
            parsing_sexp = true;
            sexp_to_parse = args[arg_index];
        }

        if bitmask == 0x80 {
            bitmask = 0x01;
            byte_idx -= 1;
        } else {
            bitmask <<= 1;
        }
    }
    if parsing_sexp {
        return Ok(sexp_to_parse);
    }
    // take bottom of stack and make (item . NIL)
    let mut backref_node = allocator.new_pair(args[0], NodePtr::NIL)?;
    if arg_index == 0 {
        return Ok(backref_node);
    }
    // for the rest of items starting from last + 1 in stack
    for x in args.iter().take(arg_index + 1).skip(1) {
        backref_node = allocator.new_pair(*x, backref_node)?;
    }
    Ok(backref_node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    use hex::FromHex;

    #[rstest]
    // ("foobar" "foobar")
    #[case(
        "ff86666f6f626172ff86666f6f62617280",
        "9148834131750904c023598bed28db269bdb29012514579e723d63e27829bcba"
    )]
    // ("foobar" "foobar")
    #[case(
        "ff86666f6f626172fe01",
        "9148834131750904c023598bed28db269bdb29012514579e723d63e27829bcba"
    )]
    // ((1 2 3 4) 1 2 3 4)
    #[case(
        "ffff01ff02ff03ff0480ff01ff02ff03ff0480",
        "028c16eb4fec600e6153d8dde60eb3916d13d0dc446b5cd7936a1248f8963bf8"
    )]
    // ((1 2 3 4) 1 2 3 4)
    #[case(
        "ffff01ff02ff03ff0480fe02",
        "028c16eb4fec600e6153d8dde60eb3916d13d0dc446b5cd7936a1248f8963bf8"
    )]
    // `(((((a_very_long_repeated_string . 1) .  (2 . 3)) . ((4 . 5) .  (6 . 7))) . (8 . 9)) 10 a_very_long_repeated_string)`
    #[case(
        "ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff04\
         05ff0607ff0809ff0aff9b615f766572795f6c6f6e675f72657065617465645f737472696e6780",
        "e23c73777f814e8a4e2785487b272b8b22ddaded1f7cfb808b43f1148602882f"
    )]
    #[case("ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff0405ff0607ff0809ff0afffe4180", "e23c73777f814e8a4e2785487b272b8b22ddaded1f7cfb808b43f1148602882f")]
    fn test_deserialize_with_backrefs(
        #[case] serialization_as_hex: &str,
        #[case] expected_hash_as_hex: &str,
    ) {
        use crate::serde::object_cache::{treehash, ObjectCache};
        let buf = Vec::from_hex(serialization_as_hex).unwrap();
        let mut allocator = Allocator::new();
        let node = node_from_bytes_backrefs(&mut allocator, &buf).unwrap();
        let old_node = node_from_bytes_backrefs_old(&mut allocator, &buf).unwrap();
        let mut oc = ObjectCache::new(treehash);
        let calculated_hash = oc.get_or_calculate(&allocator, &node, None).unwrap();
        let ch: &[u8] = calculated_hash;
        let expected_hash: Vec<u8> = Vec::from_hex(expected_hash_as_hex).unwrap();
        assert_eq!(expected_hash, ch);
        let calculated_hash = oc.get_or_calculate(&allocator, &old_node, None).unwrap();
        let ch: &[u8] = calculated_hash;
        assert_eq!(expected_hash, ch);
    }

    #[rstest]
    // ("foobar" "foobar")
    // no-backrefs
    #[case("ff86666f6f626172ff86666f6f62617280", &[])]
    // ("foobar" "foobar")
    // with back-refs
    #[case("ff86666f6f626172fe01", &["ff86666f6f62617280"])]
    // ((1 2 3 4) 1 2 3 4)
    // no-backrefs
    #[case("ffff01ff02ff03ff0480ff01ff02ff03ff0480", &[])]
    // ((1 2 3 4) 1 2 3 4)
    // with back-refs
    #[case("ffff01ff02ff03ff0480fe02", &["ff01ff02ff03ff0480"])]
    // `(((((a_very_long_repeated_string . 1) .  (2 . 3)) . ((4 . 5) .  (6 . 7))) . (8 . 9)) 10 a_very_long_repeated_string)`
    // no-backrefs
    #[case("ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff04\
         05ff0607ff0809ff0aff9b615f766572795f6c6f6e675f72657065617465645f737472696e6780", &[])]
    // with back-refs
    #[case("ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff0405ff0607ff0809ff0afffe4180",
        &["9b615f766572795f6c6f6e675f72657065617465645f737472696e67"])]
    fn test_deserialize_with_backrefs_record(
        #[case] serialization_as_hex: &str,
        #[case] expected_backrefs: &[&'static str],
    ) {
        use crate::serde::node_to_bytes;
        let buf = Vec::from_hex(serialization_as_hex).unwrap();
        let mut allocator = Allocator::new();
        let (_node, backrefs) = node_from_bytes_backrefs_record(&mut allocator, &buf)
            .expect("node_from_bytes_backrefs_records");
        println!("backrefs: {:?}", backrefs);
        assert_eq!(backrefs.len(), expected_backrefs.len());

        let expected_backrefs =
            HashSet::<String>::from_iter(expected_backrefs.iter().map(|s| s.to_string()));
        let backrefs = HashSet::from_iter(
            backrefs
                .iter()
                .map(|br| hex::encode(node_to_bytes(&allocator, *br).expect("node_to_bytes"))),
        );

        assert_eq!(backrefs, expected_backrefs);
    }
}
