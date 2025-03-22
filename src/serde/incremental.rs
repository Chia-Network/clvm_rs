// Serialization with "back-references" of an incrementally built CLVM structure

use std::io;
use std::io::{Cursor, Write};

use super::write_atom::write_atom;
use crate::allocator::{Allocator, NodePtr, SExp};
use crate::serde::{TreeCache, TreeCacheCheckpoint};

const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

#[derive(PartialEq, Eq, Clone)]
enum ReadOp {
    Parse,
    Cons(NodePtr),
}

pub struct Serializer {
    read_op_stack: Vec<ReadOp>,
    write_stack: Vec<NodePtr>,
    tree_cache: TreeCache,
    output: Cursor<Vec<u8>>,
}

#[derive(Clone)]
pub struct UndoState {
    read_op_stack: Vec<ReadOp>,
    write_stack: Vec<NodePtr>,
    tree_cache: TreeCacheCheckpoint,
    output_position: u64,
}

/// The state to allow incrementally serializing CLVM structures with back-refs
/// The compression cannot "see through" the sentinel node, so some compression
/// opportunities may be missed when serializing and compressing incrementally.
impl Serializer {
    pub fn new(sentinel: Option<NodePtr>) -> Self {
        Self {
            read_op_stack: vec![ReadOp::Parse],
            write_stack: vec![],
            tree_cache: TreeCache::new(sentinel),
            output: Cursor::new(vec![]),
        }
    }

    /// Resume serializing from the most recent sentinel node (or from the
    /// beginning if this is the first call. Returns true when we're done
    /// serializing. i.e. no sentinel token was encountered. Once this function
    /// returns true, it may not be called again.
    pub fn add(&mut self, a: &Allocator, node: NodePtr) -> io::Result<(bool, UndoState)> {
        // once we're done serializing (i.e. there was no sentinel in the last
        // call to add()), we can't resume
        assert!(!self.read_op_stack.is_empty());

        let undo_state = UndoState {
            read_op_stack: self.read_op_stack.clone(),
            write_stack: self.write_stack.clone(),
            tree_cache: self.tree_cache.undo_state(),
            output_position: self.output.position(),
        };
        self.tree_cache.update(a, node);
        self.write_stack.push(node);

        while let Some(node_to_write) = self.write_stack.pop() {
            if Some(node_to_write) == self.tree_cache.sentinel_node {
                // we're not done serializing yet, we're stopping, and the
                // caller will call add() again with the node to serialize
                // here
                return Ok((false, undo_state));
            }
            let op = self.read_op_stack.pop();
            assert!(op == Some(ReadOp::Parse));

            match self.tree_cache.find_path(node_to_write) {
                Some(path) => {
                    self.output.write_all(&[BACK_REFERENCE])?;
                    write_atom(&mut self.output, &path)?;
                    self.tree_cache.push(node_to_write);
                }
                None => match a.sexp(node_to_write) {
                    SExp::Pair(left, right) => {
                        self.output.write_all(&[CONS_BOX_MARKER])?;
                        self.write_stack.push(right);
                        self.write_stack.push(left);
                        self.read_op_stack.push(ReadOp::Cons(node_to_write));
                        self.read_op_stack.push(ReadOp::Parse);
                        self.read_op_stack.push(ReadOp::Parse);
                    }
                    SExp::Atom => {
                        let atom = a.atom(node_to_write);
                        write_atom(&mut self.output, atom.as_ref())?;
                        self.tree_cache.push(node_to_write);
                    }
                },
            }
            while let Some(ReadOp::Cons(node)) = self.read_op_stack.last() {
                let node = *node;
                self.read_op_stack.pop();
                self.tree_cache.pop2_and_cons(node);
            }
        }

        Ok((true, undo_state))
    }

    pub fn restore(&mut self, state: UndoState) {
        self.read_op_stack = state.read_op_stack;
        self.write_stack = state.write_stack;
        self.tree_cache.restore(state.tree_cache);
        self.output.set_position(state.output_position);
        self.output
            .get_mut()
            .truncate(state.output_position as usize);
    }

    pub fn size(&self) -> u64 {
        self.output.position()
    }

    /// Returns a reference to the internal serialization buffer. If add() has
    /// not yet returned true, it will return an incomplete/invalid
    /// serialization.
    pub fn get_ref(&self) -> &Vec<u8> {
        self.output.get_ref()
    }

    /// It's only valid to convert to the inner serialized form once
    /// serialization is complete. i.e. after add() returns true.
    pub fn into_inner(self) -> Vec<u8> {
        assert!(self.read_op_stack.is_empty());
        self.output.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::{
        node_from_bytes, node_from_bytes_backrefs, node_to_bytes, node_to_bytes_backrefs,
    };
    use hex_literal::hex;

    #[test]
    fn test_simple_incremental() {
        let mut a = Allocator::new();

        let sentinel = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();
        // ((1 . 2) . (3 . 4))
        let item = node_from_bytes(&mut a, &hex!("ffff0102ff0304")).unwrap();
        let list = a.new_pair(item, sentinel).unwrap();

        let mut ser = Serializer::new(Some(sentinel));
        let mut size = ser.size();
        for _ in 0..10 {
            // this keeps returning false because we encounter a sentinel
            let (done, _) = ser.add(&a, list).unwrap();
            assert!(!done);
            assert!(ser.size() > size);
            size = ser.size();
        }

        // this returns true because we're done now
        let (done, _) = ser.add(&a, NodePtr::NIL).unwrap();
        assert!(done);

        let output = ser.into_inner();
        assert_eq!(
            hex::encode(&output),
            "ffffff0102ff0304fffe02fffe02fffe02fffe02fffe02fffe02fffe02fffe02fffe0280"
        );
        let parsed = node_from_bytes_backrefs(&mut a, &output).unwrap();

        // serializing (and compressing) incrementally can't "see through" the
        // sentinel (or suspension points), since we don't have that part of the
        // tree yet. Therefore, the compression can't back-ref all
        // possible options. Compressing the whole tree in one step, we'll get
        // better compression
        let round_trip = node_to_bytes_backrefs(&a, parsed).unwrap();
        assert_eq!(
            hex::encode(&round_trip),
            "ffffff0102ff0304fffe02fffe02fffe02fffe02fe01"
        );

        // this is the uncompressed representation
        let round_trip = node_to_bytes(&a, parsed).unwrap();
        assert_eq!(hex::encode(&round_trip), "ffffff0102ff0304ffffff0102ff0304ffffff0102ff0304ffffff0102ff0304ffffff0102ff0304ffffff0102ff0304ffffff0102ff0304ffffff0102ff0304ffffff0102ff0304ffffff0102ff030480");
    }

    #[test]
    fn test_incremental() {
        let mut a = Allocator::new();

        let sentinel = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();

        // ((1 . <sentinel>) . (3 . "foobar"))
        let node1 = a.new_small_number(1).unwrap();
        let node2 = a.new_pair(node1, sentinel).unwrap();
        let node3 = a.new_small_number(3).unwrap();
        let node4 = a.new_atom(b"foobar").unwrap();
        let node5 = a.new_pair(node3, node4).unwrap();
        let item = a.new_pair(node2, node5).unwrap();

        let mut ser = Serializer::new(Some(sentinel));
        let mut size = ser.size();

        let (done, _) = ser.add(&a, item).unwrap();
        assert!(!done);
        assert!(ser.size() > size);
        size = ser.size();

        // ((1 . <sentinel>) . (3 . "barfoo"))
        let node1 = a.new_small_number(1).unwrap();
        let node2 = a.new_pair(node1, sentinel).unwrap();
        let node3 = a.new_small_number(3).unwrap();
        let node4 = a.new_atom(b"barfoo").unwrap();
        let node5 = a.new_pair(node3, node4).unwrap();
        let item = a.new_pair(node2, node5).unwrap();

        for _ in 0..10 {
            // this keeps returning false because we encounter a sentinel
            let (done, _) = ser.add(&a, item).unwrap();
            assert!(!done);
            assert!(ser.size() > size);
            size = ser.size();
        }

        // this returns true because we're done now
        let (done, _) = ser.add(&a, NodePtr::NIL).unwrap();
        assert!(done);

        // The "foobar" atom is serialized as 86666f6f626172
        // and "barfoo" as 86626172666f6f
        let output = ser.into_inner();
        assert_eq!(hex::encode(&output), "ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff0180ff0386626172666f6ffe0efe0efe0efe0efe0efe0efe0efe0efe0eff0386666f6f626172");
        let parsed = node_from_bytes_backrefs(&mut a, &output).unwrap();

        // serializing (and compressing) incrementally can't "see through" the
        // sentinel (or suspension points), since we don't have that part of the
        // tree yet. Therefore, the compression can't back-ref all
        // possible options. Compressing the whole tree in one step, we'll get
        // better compression
        let round_trip = node_to_bytes_backrefs(&a, parsed).unwrap();
        assert_eq!(hex::encode(&round_trip), "ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff0180ff0386626172666f6ffe0efe0efe0efe0efe0efe0efe0efe0efe0eff0386666f6f626172");

        // this is the uncompressed representation
        let round_trip = node_to_bytes(&a, parsed).unwrap();
        assert_eq!(hex::encode(&round_trip), "ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff01ffff0180ff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386626172666f6fff0386666f6f626172");
    }

    #[test]
    fn test_restore() {
        let mut a = Allocator::new();

        let sentinel = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();
        // ((1 . 2) . (3 . 4))
        let item = node_from_bytes(&mut a, &hex!("ffff0102ff0304")).unwrap();
        let list = a.new_pair(item, sentinel).unwrap();

        let mut ser = Serializer::new(Some(sentinel));
        let (done, _) = ser.add(&a, list).unwrap();
        assert!(!done);
        assert_eq!(ser.size(), 8);
        assert_eq!(hex::encode(ser.get_ref()), "ffffff0102ff0304");

        let (done, state) = ser.add(&a, NodePtr::NIL).unwrap();
        assert!(done);
        assert_eq!(ser.size(), 9);
        assert_eq!(hex::encode(ser.get_ref()), "ffffff0102ff030480");

        ser.restore(state.clone());

        assert_eq!(ser.size(), 8);
        assert_eq!(hex::encode(ser.get_ref()), "ffffff0102ff0304");

        let (done, _) = ser.add(&a, item).unwrap();
        assert!(done);

        assert_eq!(ser.size(), 10);
        assert_eq!(hex::encode(ser.get_ref()), "ffffff0102ff0304fe02");

        ser.restore(state);

        let item = a.new_small_number(1337).unwrap();

        let (done, _) = ser.add(&a, item).unwrap();

        assert!(done);
        assert_eq!(ser.size(), 11);
        assert_eq!(hex::encode(ser.get_ref()), "ffffff0102ff0304820539");

        let output = ser.into_inner();
        assert_eq!(hex::encode(&output), "ffffff0102ff0304820539");
    }

    #[test]
    fn test_incremental_restore() {
        let mut a = Allocator::new();

        let sentinel = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();
        // ((0x000000000000 . 0x111111111111) . (0x222222222222 . 0x333333333333))
        let item = node_from_bytes(
            &mut a,
            &hex!("ffff8600000000000086111111111111ff8622222222222286333333333333"),
        )
        .unwrap();
        let item1 = a.new_pair(item, sentinel).unwrap();

        // ((0x111111111111 . 0x000000000000) . (0x222222222222 . 0x333333333333))
        let item = node_from_bytes(
            &mut a,
            &hex!("ffff8611111111111186000000000000ff8622222222222286333333333333"),
        )
        .unwrap();
        let item2 = a.new_pair(item, sentinel).unwrap();

        // ((0x000000000000 . 0x111111111111) . (0x333333333333 . 0x222222222222))
        let item = node_from_bytes(
            &mut a,
            &hex!("ffff8600000000000086111111111111ff8633333333333386222222222222"),
        )
        .unwrap();
        let item3 = a.new_pair(item, sentinel).unwrap();

        // add item1, item2, item3
        // restore to after item1
        // add item3, item2
        // terminate the list
        let mut ser = Serializer::new(Some(sentinel));
        let (done, _) = ser.add(&a, item1).unwrap();
        assert!(!done);
        println!("{}", hex::encode(ser.get_ref()));
        let (done, restore_state) = ser.add(&a, item2).unwrap();
        assert!(!done);
        println!("{}", hex::encode(ser.get_ref()));
        let (done, _) = ser.add(&a, item3).unwrap();
        assert!(!done);
        println!("{}", hex::encode(ser.get_ref()));
        println!("restore");
        ser.restore(restore_state);
        println!("{}", hex::encode(ser.get_ref()));

        let (done, _) = ser.add(&a, item3).unwrap();
        assert!(!done);
        println!("{}", hex::encode(ser.get_ref()));
        let (done, _) = ser.add(&a, item2).unwrap();
        assert!(!done);
        println!("{}", hex::encode(ser.get_ref()));

        let (done, _) = ser.add(&a, NodePtr::NIL).unwrap();
        assert!(done);
        println!("{}", hex::encode(ser.get_ref()));

        let output = ser.into_inner();

        {
            let mut a = Allocator::new();
            let result = node_from_bytes_backrefs(&mut a, &output).expect("invalid serialization");
            let roundtrip = node_to_bytes(&a, result).expect("failed to serialize");
            assert_eq!(
                hex::encode(roundtrip),
                "
            ff
              ff
                ff
                  86000000000000
                  86111111111111
                ff
                  86222222222222
                  86333333333333
              ff
                ff
                  ff
                    86000000000000
                    86111111111111
                  ff
                    86333333333333
                    86222222222222
                ff
                  ff
                    ff
                      86111111111111
                      86000000000000
                    ff
                      86222222222222
                      86333333333333
                80"
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
            );
        }

        assert_eq!(
            hex::encode(output),
            "
        ff
          ff
            ff
              86000000000000
              86111111111111
            ff
              86222222222222
              86333333333333
          ff
            ff
              fe04
            ff
              fe1d
              fe2b
          ff
            ff
              ff
                fe0c
                fe11
              fe1b
            80"
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
        );
    }
}
