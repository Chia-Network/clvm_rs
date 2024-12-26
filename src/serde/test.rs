use hex::FromHex;

use crate::allocator::Allocator;
use crate::serde::{
    node_from_bytes, node_from_bytes_backrefs, node_to_bytes, node_to_bytes_backrefs, Serializer,
};

fn check_round_trip(obj_ser_br_hex: &str, serializer_output: Option<&str>) {
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

    // now reserialize with back-refs using the incremental serializer
    let mut allocator = Allocator::new();
    let obj = node_from_bytes(&mut allocator, &obj_ser_no_br_1).unwrap();

    let mut serializer = Serializer::new(None);
    let (done, _) = serializer.add(&allocator, obj).unwrap();
    assert!(done);
    let obj_ser_br_2 = serializer.into_inner();

    // and compare to original
    assert_eq!(obj_ser_br, obj_ser_br_1);

    // Serializer uses a different implementation that takes some short-cuts.
    // Specifically, it doesn't generate references to the parse stack itself
    match serializer_output {
        Some(expect) => {
            assert_eq!(expect, hex::encode(obj_ser_br_2));
        }
        None => {
            assert_eq!(obj_ser_br_1, obj_ser_br_2);
            assert_eq!(obj_ser_br, obj_ser_br_2);
        }
    }
}

#[test]
fn test_round_trip() {
    let check = check_round_trip;
    check("01", None); // 1
    check("ff83666f6f83626172", None); // (foo . bar)
    check("ff83666f6fff8362617280", None); // (foo bar)
    check("ffff0102ff0304", None); // ((1 . 2) . (3 . 4))
    check("ff01ff02ff03ff04ff05ff0680", None); // (1 2 3 4 5 6)
    check("ff83666f6ffe02", None); // (foo . foo)

    // (long string of long text string)
    check(
        "ff846c6f6e67ff86737472696e67ff826f66fffe0bff8474657874fffe1780",
        None,
    );

    /*
    (foo (foo) ((foo) foo) (((foo) foo) (foo) foo) ((((foo) foo) (foo) foo) ((foo) foo)
        (foo) foo) (((((foo) foo) (foo) foo) ((foo) foo) (foo) foo) (((foo) foo) (foo) foo)
        ((foo) foo) (foo) foo) ((((((foo) foo) (foo) foo) ((foo) foo) (foo) foo) (((foo) foo)
        (foo) foo) ((foo) foo) (foo) foo) ((((foo) foo) (foo) foo) ((foo) foo) (foo) foo)
        (((foo) foo) (foo) foo) ((foo) foo) (foo) foo))
    */

    // These back-references point directly to the parse stack. The Serializer
    // doesn't generate back references like that, so it will only round-trip
    // with node_to_bytes_backrefs()
    check(
        "ff83666f6ffffe01fffe01fffe01fffe01fffe01fffe0180",
        Some("ff83666f6ffffffe0280fffffe02fe02fffffe02fe02fffffe02fe02fffffe02fe02fffffe02fe0280"),
    );
}
