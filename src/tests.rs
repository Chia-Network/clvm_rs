use super::allocator::{Allocator, NodePtr, SExp};
use super::node::Node;
use super::serialize::node_from_bytes;
use super::serialize::node_to_bytes;

impl<'a> PartialEq for Node<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self.sexp(), other.sexp()) {
            (SExp::Pair(l0, l1), SExp::Pair(r0, r1)) => {
                self.with_node(l0) == self.with_node(r0) && self.with_node(l1) == self.with_node(r1)
            }
            (SExp::Atom(l0), SExp::Atom(r0)) => self.allocator.buf(&l0) == self.allocator.buf(&r0),
            _ => false,
        }
    }
}

fn test_serialize_roundtrip(a: &mut Allocator, n: NodePtr) {
    let vec = node_to_bytes(&Node::new(a, n.clone())).unwrap();
    let n0 = node_from_bytes(a, &vec).unwrap();
    let n1 = Node::new(a, n0);
    assert_eq!(Node::new(a, n), n1);
}

#[test]
fn test_roundtrip() {
    let mut a = Allocator::new();
    let n = a.null();
    test_serialize_roundtrip(&mut a, n);

    let n = a.one();
    test_serialize_roundtrip(&mut a, n);

    let n = a.new_atom(&[1_u8, 2_u8, 3_u8]).unwrap();
    test_serialize_roundtrip(&mut a, n);

    let a1 = a.new_atom(&[1_u8, 2_u8, 3_u8]).unwrap();
    let a2 = a.new_atom(&[4_u8, 5_u8, 6_u8]).unwrap();
    let p = a.new_pair(a1, a2).unwrap();
    test_serialize_roundtrip(&mut a, p);

    for idx in 0..=255 {
        let n = a.new_atom(&[idx]).unwrap();
        test_serialize_roundtrip(&mut a, n);
    }

    // large blob
    let mut buf = Vec::<u8>::new();
    buf.resize(1000000, 0_u8);
    let n = a.new_atom(&buf).unwrap();
    test_serialize_roundtrip(&mut a, n);

    // deep tree
    let mut prev = a.null();
    for _ in 0..=4000 {
        prev = a.new_pair(a.one(), prev).unwrap();
    }
    test_serialize_roundtrip(&mut a, prev);

    // deep reverse tree
    let mut prev = a.null();
    for _ in 0..=4000 {
        let n = a.one();
        prev = a.new_pair(prev, n).unwrap();
    }
    test_serialize_roundtrip(&mut a, prev);
}

#[test]
fn test_serialize_blobs() {
    let mut a = Allocator::new();

    // null
    let n = Node::new(&a, a.null());
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x80]);

    // one
    let n = Node::new(&a, a.one());
    assert_eq!(node_to_bytes(&n).unwrap(), &[1]);

    // single byte
    let atom = a.new_atom(&[128]).unwrap();
    let n = Node::new(&a, atom);
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x81, 128]);
    let n = n.node;
    test_serialize_roundtrip(&mut a, n);

    // two bytes
    let atom = a.new_atom(&[0x10, 0xff]).unwrap();
    let n = Node::new(&a, atom);
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x82, 0x10, 0xff]);
    let n = n.node;
    test_serialize_roundtrip(&mut a, n);

    // three bytes
    let atom = a.new_atom(&[0xff, 0x10, 0xff]).unwrap();
    let n = Node::new(&a, atom);
    assert_eq!(node_to_bytes(&n).unwrap(), &[0x83, 0xff, 0x10, 0xff]);
    let n = n.node;
    test_serialize_roundtrip(&mut a, n);
}

#[test]
fn test_serialize_lists() {
    let mut a = Allocator::new();

    // null
    let n = a.null();
    assert_eq!(node_to_bytes(&Node::new(&a, n)).unwrap(), &[0x80]);

    // one item
    let n = a.new_pair(a.one(), n).unwrap();
    assert_eq!(node_to_bytes(&Node::new(&a, n)).unwrap(), &[0xff, 1, 0x80]);

    // two items
    let n = a.new_pair(a.one(), n).unwrap();
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 1, 0xff, 1, 0x80]
    );
    test_serialize_roundtrip(&mut a, n);

    // three items
    let n = a.new_pair(a.one(), n).unwrap();
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 1, 0xff, 1, 0xff, 1, 0x80]
    );
    test_serialize_roundtrip(&mut a, n);

    // a backwards list
    let n = a.one();
    let n = a.new_pair(n, a.one()).unwrap();
    let n = a.new_pair(n, a.one()).unwrap();
    let n = a.new_pair(n, a.one()).unwrap();
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 0xff, 0xff, 1, 1, 1, 1]
    );
    test_serialize_roundtrip(&mut a, n);
}

#[test]
fn test_serialize_tree() {
    let mut a = Allocator::new();

    let a1 = a.new_atom(&[1]).unwrap();
    let a2 = a.new_atom(&[2]).unwrap();
    let a3 = a.new_atom(&[3]).unwrap();
    let a4 = a.new_atom(&[4]).unwrap();
    let l = a.new_pair(a1, a2).unwrap();
    let r = a.new_pair(a3, a4).unwrap();
    let n = a.new_pair(l, r).unwrap();
    assert_eq!(
        node_to_bytes(&Node::new(&a, n)).unwrap(),
        &[0xff, 0xff, 1, 2, 0xff, 3, 4]
    );
    test_serialize_roundtrip(&mut a, n);
}
