use crate::node::{Allocator, Node};

use uint::U256;
pub type Number = U256;

pub fn node_from_number<T, U>(allocator: &dyn Allocator<T, U>, item: Number) -> T {
    // BRAIN DAMAGE: make it minimal by removing leading zeros
    let mut bytes: Vec<u8> = vec![0; 32];
    item.to_big_endian(&mut bytes);
    let mut slice = bytes.as_slice();
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    allocator.blob_u8(&slice)
}

impl From<&Node> for Option<Number> {
    fn from(item: &Node) -> Self {
        let v = item.atom()?;
        let len = v.len();
        if len == 0 {
            return Some(0.into());
        }
        if len <= 32 {
            // TODO: check that it's a minimal number
            return Some(U256::from_big_endian(&v));
        }
        None
    }
}
