use crate::allocator::{Allocator, NodePtr};
use crate::node::Node;
use crate::reduction::EvalErr;

use num_bigint::BigInt;
pub type Number = BigInt;

pub fn ptr_from_number(allocator: &mut Allocator, item: &Number) -> Result<NodePtr, EvalErr> {
    let bytes: Vec<u8> = item.to_signed_bytes_be();
    let mut slice = bytes.as_slice();

    // make number minimal by removing leading zeros
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    allocator.new_atom(slice)
}

impl From<&Node<'_>> for Option<Number> {
    fn from(item: &Node) -> Self {
        let v: &[u8] = item.atom()?;
        Some(number_from_u8(v))
    }
}

pub fn number_from_u8(v: &[u8]) -> Number {
    let len = v.len();
    if len == 0 {
        0.into()
    } else {
        Number::from_signed_bytes_be(v)
    }
}

#[test]
fn test_ptr_from_number() {
    let mut a = Allocator::new();

    // 0 is encoded as an empty string
    let num = number_from_u8(&[0]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "0");
    assert_eq!(a.atom(ptr).len(), 0);

    let num = number_from_u8(&[1]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "1");
    assert_eq!(&[1], &a.atom(ptr));

    // leading zeroes are redundant
    let num = number_from_u8(&[0, 0, 0, 1]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "1");
    assert_eq!(&[1], &a.atom(ptr));

    let num = number_from_u8(&[0x00, 0x00, 0x80]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "128");
    assert_eq!(&[0x00, 0x80], &a.atom(ptr));

    // A leading zero is necessary to encode a positive number with the
    // penultimate byte's most significant bit set
    let num = number_from_u8(&[0x00, 0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "255");
    assert_eq!(&[0x00, 0xff], &a.atom(ptr));

    let num = number_from_u8(&[0x7f, 0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "32767");
    assert_eq!(&[0x7f, 0xff], &a.atom(ptr));

    // the first byte is redundant, it's still -1
    let num = number_from_u8(&[0xff, 0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "-1");
    assert_eq!(&[0xff], &a.atom(ptr));

    let num = number_from_u8(&[0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "-1");
    assert_eq!(&[0xff], &a.atom(ptr));

    let num = number_from_u8(&[0x00, 0x80, 0x00]);
    assert_eq!(format!("{}", num), "32768");
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(&[0x00, 0x80, 0x00], &a.atom(ptr));

    let num = number_from_u8(&[0x00, 0x40, 0x00]);
    assert_eq!(format!("{}", num), "16384");
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(&[0x40, 0x00], &a.atom(ptr));
}
