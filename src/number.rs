use crate::allocator::Allocator;
use crate::node::Node;

use num_bigint::BigInt;
pub type Number = BigInt;

pub fn node_from_number<'a, T>(allocator: &'a dyn Allocator<T>, item: &Number) -> Node<'a, T> {
    // BRAIN DAMAGE: make it minimal by removing leading zeros
    let bytes: Vec<u8> = item.to_signed_bytes_be();
    let mut slice = bytes.as_slice();
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    Node::new(allocator, allocator.blob_u8(&slice))
}

impl<T> From<&Node<'_, T>> for Option<Number> {
    fn from(item: &Node<T>) -> Self {
        let v: &[u8] = &item.atom()?;
        Some(number_from_u8(v))
    }
}

pub fn number_from_u8(v: &[u8]) -> Number {
    let len = v.len();
    if len == 0 {
        0.into()
    } else {
        Number::from_signed_bytes_be(&v)
    }
}
