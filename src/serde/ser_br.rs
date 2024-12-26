// Serialization with "back-references"

use std::io;

use crate::allocator::{Allocator, NodePtr};
use crate::serde::incremental::Serializer;

pub fn node_to_bytes_backrefs(a: &Allocator, node: NodePtr) -> io::Result<Vec<u8>> {
    let mut ser = Serializer::new();
    ser.add(a, node, None)?;
    Ok(ser.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::node_to_bytes_backrefs;

    #[test]
    fn test_serialize_limit() {
        let mut a = Allocator::new();

        let leaf = a.new_atom(&[1, 2, 3, 4, 5]).unwrap();
        let l1 = a.new_pair(leaf, leaf).unwrap();
        let l2 = a.new_pair(l1, l1).unwrap();
        let l3 = a.new_pair(l2, l2).unwrap();

        let expected = &[255, 255, 255, 133, 1, 2, 3, 4, 5, 254, 2, 254, 2, 254, 2];

        assert_eq!(node_to_bytes_backrefs(&a, l3).unwrap(), expected);
    }
}
