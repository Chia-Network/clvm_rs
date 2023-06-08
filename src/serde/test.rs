use hex::FromHex;

use crate::allocator::Allocator;

use crate::serde::de::node_from_bytes;
use crate::serde::de_br::node_from_bytes_backrefs;
use crate::serde::ser::node_to_bytes;
use crate::serde::ser_br::node_to_bytes_backrefs;

fn check_round_trip(obj_ser_br_hex: &str) {
    // serialized with br => obj => serialized no br =(allow_br)=> obj => serialized w br

    // serialized object, with back-refs
    let obj_ser_br = <Vec<u8>>::from_hex(obj_ser_br_hex).unwrap();

    // turn into serialized object with no back-refs
    let mut allocator = Allocator::new();
    let obj = node_from_bytes_backrefs(&mut allocator, &obj_ser_br).unwrap();

    let obj_ser_no_br_1 = node_to_bytes(&allocator, obj).unwrap();

    // deserialize using `node_from_bytes_backrefs` (even though there are no backrefs)
    // and reserialized without back-refs
    let mut allocator = Allocator::new();
    let obj = node_from_bytes_backrefs(&mut allocator, &obj_ser_no_br_1).unwrap();

    let obj_ser_no_br_2 = node_to_bytes(&allocator, obj).unwrap();

    // compare both reserializations (without back-refs)
    assert_eq!(obj_ser_no_br_1, obj_ser_no_br_2);

    // now reserialize with back-refs
    let mut allocator = Allocator::new();
    let obj = node_from_bytes(&mut allocator, &obj_ser_no_br_1).unwrap();

    let obj_ser_br_1 = node_to_bytes_backrefs(&allocator, obj).unwrap();

    // and compare to original
    assert_eq!(obj_ser_br, obj_ser_br_1);
}

#[test]
fn test_round_trip() {
    let check = check_round_trip;
    check("01"); // 1
    check("ff83666f6f83626172"); // (foo . bar)
    check("ff83666f6fff8362617280"); // (foo bar)
    check("ffff0102ff0304"); // ((1 . 2) . (3 . 4))
    check("ff01ff02ff03ff04ff05ff0680"); // (1 2 3 4 5 6)
    check("ff83666f6ffe02"); // (foo . foo)

    // (long string of long text string)
    check("ff846c6f6e67ff86737472696e67ff826f66fffe0bff8474657874fffe1780");

    /*
    (foo (foo) ((foo) foo) (((foo) foo) (foo) foo) ((((foo) foo) (foo) foo) ((foo) foo)
        (foo) foo) (((((foo) foo) (foo) foo) ((foo) foo) (foo) foo) (((foo) foo) (foo) foo)
        ((foo) foo) (foo) foo) ((((((foo) foo) (foo) foo) ((foo) foo) (foo) foo) (((foo) foo)
        (foo) foo) ((foo) foo) (foo) foo) ((((foo) foo) (foo) foo) ((foo) foo) (foo) foo)
        (((foo) foo) (foo) foo) ((foo) foo) (foo) foo))
    */

    check("ff83666f6ffffe01fffe01fffe01fffe01fffe01fffe0180");
}
