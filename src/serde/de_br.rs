use std::collections::HashSet;
use std::io;
use std::io::{Cursor, Read};

use crate::allocator::{Allocator, NodePtr};
use crate::traverse_path::traverse_path_with_vec;

use super::parse_atom::{parse_atom, parse_path};

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
                    let reduction = traverse_path_with_vec(allocator, path, &values)?;
                    let back_reference = reduction.1;
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

pub fn node_from_bytes_backrefs(allocator: &mut Allocator, b: &[u8]) -> io::Result<NodePtr> {
    let mut buffer = Cursor::new(b);
    node_from_stream_backrefs(allocator, &mut buffer, |_node| {})
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
        let mut oc = ObjectCache::new(treehash);
        let calculated_hash = oc.get_or_calculate(&allocator, &node, None).unwrap();
        let ch: &[u8] = calculated_hash;
        let expected_hash: Vec<u8> = Vec::from_hex(expected_hash_as_hex).unwrap();
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
