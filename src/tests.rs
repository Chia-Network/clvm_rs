use super::allocator::Allocator;
use super::int_allocator::IntAllocator;
use super::node;
use super::serialize::node_from_bytes;
use super::serialize::node_to_bytes;

type Node<'a> = node::Node<'a, IntAllocator>;

fn test_serialize_roundtrip(n: &Node) {
    let vec = node_to_bytes(n).unwrap();
    let n1: Node = Node::new(&n.allocator, node_from_bytes(n.allocator, &vec).unwrap());
    assert_eq!(*n, n1);
}

#[test]
fn test_roundtrip() {
    let a = IntAllocator::new();
    let n = Node::new(&a, a.null());
    test_serialize_roundtrip(&n);

    let n = Node::new(&a, a.one());
    test_serialize_roundtrip(&n);

    let n = Node::new(&a, a.new_atom(&[1_u8, 2_u8, 3_u8]));
    test_serialize_roundtrip(&n);

    let n = Node::new(
        &a,
        a.new_pair(
            &a.new_atom(&[1_u8, 2_u8, 3_u8]),
            &a.new_atom(&[4_u8, 5_u8, 6_u8]),
        ),
    );
    test_serialize_roundtrip(&n);

    for idx in 0..=255 {
        let n = Node::new(&a, a.new_atom(&[idx]));
        test_serialize_roundtrip(&n);
    }

    // large blob
    let mut buf = Vec::<u8>::new();
    buf.resize(1000000, 0_u8);
    let n = Node::new(&a, a.new_atom(&buf));
    test_serialize_roundtrip(&n);

    // deep tree
    let mut prev = a.null();
    for _ in 0..=4000 {
        prev = a.new_pair(&a.one(), &prev);
    }
    let n = Node::new(&a, prev);
    test_serialize_roundtrip(&n);

    // deep reverse tree
    let mut prev = a.null();
    for _ in 0..=4000 {
        prev = a.new_pair(&prev, &a.one());
    }
    let n = Node::new(&a, prev);
    test_serialize_roundtrip(&n);
}

#[test]
fn test_serialize_blobs() {
    let a = IntAllocator::new();

    // null
    let n = Node::new(&a, a.null());
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x80]);
    test_serialize_roundtrip(&n);

    // one
    let n = Node::new(&a, a.one());
    assert_eq!(node_to_bytes(&n).unwrap(), &[1]);
    test_serialize_roundtrip(&n);

    // single byte
    let n = Node::new(&a, a.new_atom(&[128]));
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x81, 128]);
    test_serialize_roundtrip(&n);

    // two bytes
    let n = Node::new(&a, a.new_atom(&[0x10, 0xff]));
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x82, 0x10, 0xff]);
    test_serialize_roundtrip(&n);

    // three bytes
    let n = Node::new(&a, a.new_atom(&[0xff, 0x10, 0xff]));
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x83, 0xff, 0x10, 0xff]);
    test_serialize_roundtrip(&n);
}

#[test]
fn test_serialize_lists() {
    let a = IntAllocator::new();

    // null
    let n = a.null();
    assert_eq!(node_to_bytes(&Node::new(&a, n)).unwrap(), &[0x80]);
    test_serialize_roundtrip(&Node::new(&a, n));

    // one item
    let n = a.new_pair(&a.one(), &n);
    assert_eq!(node_to_bytes(&Node::new(&a, n)).unwrap(), &[0xff, 1, 0x80]);
    test_serialize_roundtrip(&Node::new(&a, n));

    // two items
    let n = a.new_pair(&a.one(), &n);
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 1, 0xff, 1, 0x80]
    );
    test_serialize_roundtrip(&Node::new(&a, n));

    // three items
    let n = a.new_pair(&a.one(), &n);
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 1, 0xff, 1, 0xff, 1, 0x80]
    );
    test_serialize_roundtrip(&Node::new(&a, n));

    // a backwards list
    let n = a.one();
    let n = a.new_pair(&n, &a.one());
    let n = a.new_pair(&n, &a.one());
    let n = a.new_pair(&n, &a.one());
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 0xff, 0xff, 1, 1, 1, 1]
    );
    test_serialize_roundtrip(&Node::new(&a, n));
}

#[test]
fn test_serialize_tree() {
    let a = IntAllocator::new();

    let l = a.new_pair(&a.new_atom(&[1]), &a.new_atom(&[2]));
    let r = a.new_pair(&a.new_atom(&[3]), &a.new_atom(&[4]));
    let n = a.new_pair(&l, &r);
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 0xff, 1, 2, 0xff, 3, 4]
    );
    test_serialize_roundtrip(&Node::new(&a, n));
}

/*
fn node_from_hex<'a>(a: &'a IntAllocator, the_hex: &str) -> Node<'a> {
    let mut buffer = Cursor::new(Vec::new());
    buffer.write_all(&hex::decode(the_hex).unwrap()).unwrap();
    Node::new(a, node_from_bytes(a, &buffer.get_ref()).unwrap())
}

fn node_to_hex(node: &Node) -> String {
    hex::encode(node_to_bytes(node))
}

fn do_test_run_program(input_as_hex: &str, expected_as_hex: &str) -> () {
    let a = IntAllocator::new();
    let n = node_from_hex(&a, input_as_hex);
    println!("n = {:?}", n);
    let r = do_run_program(&n, &null);
    println!("r = {:?}", r);
    assert_eq!(node_to_hex(&r), expected_as_hex);
}

#[test]
fn test_run_program() {
}
*/
