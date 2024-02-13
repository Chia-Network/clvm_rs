#![no_main]
use libfuzzer_sys::fuzz_target;

use clvmr::{Allocator, NodePtr};

fn run_tests(a: &mut Allocator, atom1: NodePtr, data: &[u8]) {
    assert_eq!(a.atom(atom1).as_ref(), data);
    assert_eq!(a.atom_len(atom1), data.len());

    let canonical = data != [0]
        && (data.len() < 2 || data[0] != 0 || (data[1] & 0x80) != 0)
        && (data.len() < 2 || data[0] != 0xff || (data[1] & 0x80) == 0);

    // small_number
    if let Some(val) = a.small_number(atom1) {
        let atom2 = a.new_small_number(val).expect("new_small_number()");
        assert_eq!(a.atom(atom1), a.atom(atom2));
        assert_eq!(a.atom(atom2).as_ref(), data);
        assert!(a.atom_eq(atom1, atom2));
        assert_eq!(a.number(atom1), val.into());
        assert_eq!(a.number(atom2), val.into());
        assert_eq!(a.atom_len(atom2), data.len());
        assert!(canonical);
    }

    // number
    let val = a.number(atom1);

    let atom3 = a.new_number(val.clone()).expect("new_number()");

    assert_eq!(a.number(atom3), val);
    // if the atom is not in canonical integer form we don't expect it to stay
    // the same once we "launder" it through a BigInt.
    if !canonical {
        assert!(a.atom(atom3).as_ref() != data);
        assert!(a.atom_len(atom3) < data.len());
        assert!(!a.atom_eq(atom1, atom3));
    } else {
        assert_eq!(a.atom(atom3).as_ref(), data);
        assert_eq!(a.atom_len(atom3), data.len());
        assert!(a.atom_eq(atom1, atom3));
    }
}

fuzz_target!(|data: &[u8]| {
    let mut a = Allocator::new();
    let atom1 = a.new_atom(data).expect("new_atom()");
    run_tests(&mut a, atom1, data);

    let atom1 = a
        .new_concat(data.len(), &[a.nil(), atom1, a.nil()])
        .expect("new_concat()");
    run_tests(&mut a, atom1, data);
});
