use super::{ChildPos, PathBuilder};
use crate::allocator::{Allocator, NodePtr, SExp};
use crate::serde::serialized_length_atom;
use crate::serde::BitSet;
use crate::serde::RandomState;
use bumpalo::Bump;
use rand::prelude::*;
use sha1::{Digest, Sha1};
use std::collections::hash_map::Entry;
use std::collections::HashMap;

const MIN_SERIALIZED_LENGTH: u64 = 4;

type Bytes20 = [u8; 20];

fn hash_atom(salt: &[u8], blob: &[u8]) -> Bytes20 {
    let mut ctx = Sha1::default();
    ctx.update(salt);
    ctx.update(blob);
    ctx.finalize().into()
}

#[derive(Clone, Debug)]
struct NodeEntry {
    /// the tree hash of this node. It may be None if it or any of its descendants
    /// is the sentinel node, which means we can't compute the tree hash.
    tree_hash: Option<Bytes20>,
    /// a node can have an arbitrary number of parents, since they can be reused
    /// this is a list of parent nodes, followed by whether we're the left or
    /// right child. The u32 is an index into the node_entries vector.
    parents: Vec<(u32, ChildPos)>,
    /// if this node doesn't have a tree_hash, the serialized length is not
    /// valid as it cannot be computed.
    serialized_length: u64,
    /// set to non-zero, if this node is pushed onto the parse stack. Since a
    /// node can be on the stack multiple times, it's counted up every time it's
    /// pushed, and counted down every time it's popped.
    pub on_stack: u32,
}

struct PartialPath<'alloc> {
    // the path we've built so far
    path: PathBuilder<'alloc>,
    // if we're traversing the stack, this is the stack position. Note that this
    // is not an index into the stack array, it's a counter of how far away from
    // the top of the stack we are. 0 means we're at the top, and we've found
    // a path.
    // -1 means we're not traversing the stack.
    // TODO: these fields are mutually exclusive (stack_pos vs. idx and child).
    // it might be nice to use an enum, but it's not obvious that it's simpler
    // or faster
    stack_pos: i32,
    // if we're not traversing the stack, this is the next parent, and whether
    // we're coming from the left or right child
    idx: u32,
    child: ChildPos,
}

enum CacheOp {
    // node to traverse
    Traverse(NodePtr),
    // pair node
    Cons(NodePtr),
}

#[derive(Clone)]
pub struct TreeCacheCheckpoint {
    stack: Vec<u32>,
    serialized_nodes: BitSet,
    sentinel_entry: Option<u32>,
}

/// The TreeCache builds a "shadow tree" mirroring a CLVM tree but with
/// additional metadata, as well as joining identical sub trees. This is done by
/// the update() function. This data structure then supports find_path() for
/// finding back-reference paths during CLVM serialization with compression.
/// find_path() performs a reverse-search from a specified node to the top of
/// the parse stack, tracking the state of the parser.
/// For example use, see test_basic_tree() below.
#[derive(Default)]
pub struct TreeCache {
    /// caches extra metadata about a tree of nodes. The value is an index into
    /// the node_entries vector.
    node_map: HashMap<NodePtr, u32>,

    /// The metadata for all nodes in the tree. This is like a shadow tree
    /// structure to the NodePtr one. The most important difference is that
    /// identical nodes are merged, using the same NodeEntry, and additional
    /// metadata is kept, such as the tree hash.
    node_entries: Vec<NodeEntry>,

    /// maps tree-hashes to the index of the corresponding NodeEntry in the
    /// node_entries vector. For any given tree hash, we're only supposed to
    /// have a single NodeEntry. There may be multiple NodePtr referring to
    /// the same NodeEntry (if they are identical sub trees).
    atom_lookup: HashMap<Bytes20, u32, RandomState>,

    /// maps left + right child indices to the index of the pair with those
    /// children. This is the atom_lookup counterpart for pairs
    pair_lookup: HashMap<u64, u32>,

    /// When deserializing, we keep a stack of nodes we've parsed so far, this
    /// stack is maintaining that same state, since that's what back-references
    /// are pointing into.
    stack: Vec<u32>,

    /// This records which NodeEntries have been serialized so far. When we look
    /// for back-references, we can only pick nodes in this set. nodes with
    /// small serialized length are not inserted. This set is built and
    /// updated as we serialize, to ensure we only include nodes that *can* be
    /// referenced.
    serialized_nodes: BitSet,

    /// if the sentinel node is set, we can't compute the tree hashes or
    /// serialized length for this node nor any of its ancestors. When calling
    /// update(), the tree is assumed to be placed at the sentinel node in the
    /// previous call to update()
    pub sentinel_node: Option<NodePtr>,

    /// We compute hash-trees using SHA-1 in order to determine whether the
    /// trees are identical or not. To mitigate malicious SHA-1 hash collisions,
    /// we salt the hashes
    salt: [u8; 8],
}

impl TreeCache {
    pub fn new(sentinel: Option<NodePtr>) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            sentinel_node: sentinel,
            atom_lookup: HashMap::with_hasher(RandomState::default()),
            salt: rng.gen(),
            ..Default::default()
        }
    }

    pub fn undo_state(&self) -> TreeCacheCheckpoint {
        let sentinel_entry = match self.sentinel_node {
            Some(sentinel) => self.node_map.get(&sentinel).cloned(),
            None => None,
        };
        TreeCacheCheckpoint {
            stack: self.stack.clone(),
            serialized_nodes: self.serialized_nodes.clone(),
            sentinel_entry,
        }
    }

    pub fn restore(&mut self, st: TreeCacheCheckpoint) {
        for idx in &self.stack {
            self.node_entries[*idx as usize].on_stack -= 1;
        }
        #[cfg(not(debug_assertions))]
        for e in &self.node_entries {
            assert_eq!(e.on_stack, 0);
        }

        self.stack = st.stack;
        for idx in &self.stack {
            self.node_entries[*idx as usize].on_stack += 1;
        }
        self.serialized_nodes = st.serialized_nodes;
        if let Some(sentinel_entry) = st.sentinel_entry {
            self.node_map
                .insert(self.sentinel_node.unwrap(), sentinel_entry);
        }
    }

    pub fn update(&mut self, a: &Allocator, root: NodePtr) {
        let mut root_parents = Vec::<(u32, ChildPos)>::new();
        if let Some(placement) = self.sentinel_node {
            // "placement" is the sentinel node we used in the last update.
            // This position in the tree is now replaced by "root". Update
            // the node node_map to reflect this
            if let Some(idx) = self.node_map.get(&placement) {
                root_parents.append(&mut self.node_entries[*idx as usize].parents);
            }
        };

        // The first step is to compute the tree-hash and serialized length for
        // every node in the tree. However, we can't compute the hash or
        // serialized length of the sentinel node, so it and all its ancestors
        // will be blank, and not participate in the lookup.
        let mut ops = vec![CacheOp::Traverse(root)];
        // the node traversal stack. Each element is an index into node_entries
        let mut stack = Vec::<u32>::new();

        while let Some(op) = ops.pop() {
            match op {
                CacheOp::Traverse(node) => {
                    // Early exit if the node we're traversing is the sentinel
                    // node. It means we have to stop the traversal, as it's a
                    // place holder for an unknown sub tree.
                    if Some(node) == self.sentinel_node {
                        let idx = self.node_entries.len() as u32;
                        let entry = NodeEntry {
                            tree_hash: None,
                            parents: vec![],
                            serialized_length: 0,
                            on_stack: 0,
                        };
                        self.node_map.insert(node, idx);
                        self.node_entries.push(entry);
                        stack.push(idx);
                        continue;
                    }

                    let e = match self.node_map.entry(node) {
                        Entry::Occupied(e) => {
                            // If this node is already in the node_map, meaning
                            // we've already traversed it once. No need to do it
                            // again.
                            let idx = *e.get();
                            stack.push(idx);
                            continue;
                        }
                        Entry::Vacant(e) => e,
                    };

                    // traverse the node. If it's a pair, push the work
                    // onto the op stack, otherwise, hash and node_map the
                    // atom. We'll hash and node_map the pairs as we
                    // unwind.
                    if let SExp::Pair(left, right) = a.sexp(node) {
                        ops.push(CacheOp::Cons(node));
                        ops.push(CacheOp::Traverse(right));
                        ops.push(CacheOp::Traverse(left));
                        continue;
                    }
                    let buf = a.atom(node);
                    let hash = hash_atom(&self.salt, buf.as_ref());

                    // record the mapping of this node to the
                    // corresponding NodeEntry index
                    // now that we've hashed the node, it might be
                    // identical to an existing one. If so, use the
                    // same NodeEntry, otherwise, add a new one.
                    let ne = match self.atom_lookup.entry(hash) {
                        Entry::Occupied(ne) => {
                            // we already have a node with this
                            // hash
                            let idx = *ne.get();
                            e.insert(idx);
                            stack.push(idx);
                            continue;
                        }
                        Entry::Vacant(ne) => ne,
                    };
                    let idx = self.node_entries.len() as u32;
                    ne.insert(idx);
                    e.insert(idx);
                    stack.push(idx);
                    let serialized_length = serialized_length_atom(buf.as_ref());
                    self.node_entries.push(NodeEntry {
                        tree_hash: Some(hash),
                        parents: vec![],
                        serialized_length: u64::from(serialized_length),
                        on_stack: 0,
                    });
                }
                CacheOp::Cons(node) => {
                    let e = match self.node_map.entry(node) {
                        Entry::Occupied(e) => {
                            // even though node wasn't in the node_map when we pushed this
                            // CacheOp, it may be in the node_map now.
                            let idx = *e.get();
                            stack.push(idx);
                            continue;
                        }
                        Entry::Vacant(e) => e,
                    };
                    let right_idx = stack.pop().expect("empty stack") as usize;
                    let left_idx = stack.pop().expect("empty stack") as usize;

                    let left = &self.node_entries[left_idx];
                    let right = &self.node_entries[right_idx];
                    let serialized_length =
                        if left.serialized_length > 0 && right.serialized_length > 0 {
                            1 + left.serialized_length + right.serialized_length
                        } else {
                            0
                        };

                    let key: u64 = ((left_idx as u64) << 32) | (right_idx as u64);

                    // if we already have a NodeEntry, use it, otherwise add
                    // a new one
                    let idx = match self.pair_lookup.entry(key) {
                        Entry::Occupied(e) => *e.get(),
                        Entry::Vacant(e) => {
                            let idx = self.node_entries.len() as u32;
                            let entry = NodeEntry {
                                tree_hash: None,
                                parents: vec![],
                                serialized_length,
                                on_stack: 0,
                            };
                            self.node_entries.push(entry);
                            e.insert(idx);
                            idx
                        }
                    };

                    self.node_entries[left_idx]
                        .parents
                        .push((idx, ChildPos::Left));
                    self.node_entries[right_idx]
                        .parents
                        .push((idx, ChildPos::Right));
                    e.insert(idx);
                    stack.push(idx);
                }
            }
        }

        // the root node should be on the stack
        debug_assert_eq!(stack.len(), 1);

        // now that we have the NodeEntry for the root, we can update its
        // parents (if there are any). If this is not the first time we call
        // update(), we transfer the parents from the previous sentinel node to
        // this root, as that's where this tree is placed.
        let root_idx = stack[0];
        debug_assert_eq!(
            root_idx,
            *self.node_map.get(&root).expect("root not in node_map")
        );
        let root_entry = &mut self.node_entries[root_idx as usize];
        root_entry.parents.extend(root_parents);

        // allocate memory to track the new nodes
        self.serialized_nodes.extend(self.node_entries.len() as u32);
    }

    /// the push() and pop2_and_cons() functions are used to maintain the
    /// current serialization state. We need to know this to produce correct
    /// paths into this stack when creating back-references.
    pub fn push(&mut self, node: NodePtr) {
        let idx = *self.node_map.get(&node).expect("invalid node");
        let entry = &mut self.node_entries[idx as usize];
        entry.on_stack += 1;

        // serialized_length is 0 for nodes that are the sentinel or one of its
        // parents
        if entry.serialized_length >= MIN_SERIALIZED_LENGTH {
            self.serialized_nodes.visit(idx);
        }
        self.stack.push(idx);
    }

    fn pop(&mut self) {
        let idx = self.stack.pop().expect("empty stack");
        let entry = &mut self.node_entries[idx as usize];
        assert!(entry.on_stack > 0);
        entry.on_stack -= 1;
    }

    pub fn pop2_and_cons(&mut self, node: NodePtr) {
        self.pop();
        self.pop();
        self.push(node);
    }

    /// If a node with this hash already exists and is eligible to be
    /// referenced, this function returns the path (environment lookup) from the
    /// current serialization state to that tree. The serialization state is
    /// the stack of nodes currently in-flight. The bottom value in the stack is
    /// where the final tree is being collected as we parse. Nodes are eligible
    /// to be referenced after they've been serialized once. That's when they're
    /// added to the serialized_nodes set.
    pub fn find_path(&self, node: NodePtr) -> Option<Vec<u8>> {
        if node == NodePtr::NIL {
            return None;
        }
        let idx = *self.node_map.get(&node).expect("invalid node");
        if !self.serialized_nodes.is_visited(idx) {
            return None;
        };

        let entry = &self.node_entries[idx as usize];

        // if there's no serialized length for this node, it means it's the sentinel
        // node, or one of its ancestors. We can't build a path to it
        if entry.serialized_length == 0 {
            return None;
        }

        if entry.serialized_length < MIN_SERIALIZED_LENGTH {
            return None;
        }

        // this limit is 1 bit more than the longest path we're allowed to
        // produce. If we find a path of this length, we won't return it.
        let path_length_limit = (entry.serialized_length - 1).saturating_mul(8);

        // During this search (from `node` to the top of the stack) we need to
        // track all nodes we've already visited. It's critical to terminate any
        // partial path that hits an already visited node, otherwise we may end
        // up stuck in an infinite cycle. We also save time by not
        // re-considering a node via a different path, that we already know will
        // be longer than the one first visiting this node.
        let mut seen = BitSet::new(self.node_entries.len() as u32);

        let arena = Bump::new();

        // We perform a breadth-first search from the node we're finding a path
        // to, up through its parents until we find the top of the stack. Note
        // since nodes are deduplicated, they may have multiple parents.
        // We aim to have every "partial path" have the same length path, since
        // it's breadth first.
        let mut partial_paths = Vec::<PartialPath>::with_capacity(20);

        // The search from `node` to the top of the stack is essentially a
        // regular djikstra's algorithm. Instead of a priority queue of the
        // frontier of vertices, we use a flat vector of partial_paths, all
        // stepping forward in lock step. Cursor is the index into partial_paths
        // pointing to the path we're currently considering and current_length
        // indicates the length of paths that we want to consider in this pass
        // over the vector. This ensures that all partial paths move in
        // lock-step. This is important, since this algorithm rely on the
        // *first* path that reaches the target is also the shortest one.
        let mut cursor = 0;

        // this child pos represents the path terminator bit
        partial_paths.push(PartialPath {
            path: PathBuilder::default(),
            stack_pos: -1,
            idx,
            child: ChildPos::Right,
        });

        // in order to advance every partial path in lock step we only advance
        // the ones whose length is "current_length", which is incremented for every pass
        let mut current_length = 0;

        let ret: PathBuilder = loop {
            if partial_paths.is_empty() {
                return None;
            }
            if cursor == 0 && current_length > path_length_limit {
                return None;
            }
            let p = &mut partial_paths[cursor];
            if u64::from(p.path.len()) > current_length {
                cursor += 1;
                if cursor >= partial_paths.len() {
                    cursor = 0;
                    current_length += 1;
                }
                continue;
            }
            if p.stack_pos >= 0 {
                // this path is traversing the stack, not the tree nodes
                if p.stack_pos == 0 {
                    // we found the shortest path
                    break partial_paths.swap_remove(cursor).path;
                }
                p.path.push(&arena, ChildPos::Right);
                p.stack_pos -= 1;
                cursor += 1;
                if cursor >= partial_paths.len() {
                    cursor = 0;
                    current_length += 1;
                }
                continue;
            }
            if seen.visit(p.idx) {
                // if we've already visited this node, terminate this banch of
                // the search
                partial_paths.swap_remove(cursor);
                if cursor >= partial_paths.len() {
                    cursor = 0;
                    current_length += 1;
                }
                continue;
            }
            p.path.push(&arena, p.child);

            let entry = &self.node_entries[p.idx as usize];
            let idx = p.idx;

            // this search can branch if the node has parents or if it's on the
            // stack. The node being on the stack doesn't necessarily mean
            // that's the shortest path, its parent could be much further up
            // the stack for instance. We need to fork the search both to
            // follow the stack and any parent.

            // the first viable parent is a special case, where we continue
            // traversal on the "p" PartialPath
            let (remaining_parents, used_p) = if let Some(first_parent) =
                entry.parents.iter().position(|e| !seen.is_visited(e.0))
            {
                p.idx = entry.parents[first_parent].0;
                p.child = entry.parents[first_parent].1;
                (&entry.parents[(first_parent + 1)..], true)
            } else {
                (&[] as &[(u32, ChildPos)], false)
            };

            if entry.on_stack > 0 || !remaining_parents.is_empty() {
                // from now on, we can't use "p" anymore, since we're about to
                // mutate partial_paths and p is a reference into one of its
                // elements
                let mut current_path = p.path.clone(&arena);
                debug_assert_eq!(self.node_entries[idx as usize].tree_hash, entry.tree_hash);

                debug_assert!(remaining_parents.is_empty() || used_p);
                for parent in remaining_parents {
                    if !seen.is_visited(parent.0) {
                        partial_paths.push(PartialPath {
                            path: current_path.clone(&arena),
                            stack_pos: -1,
                            idx: parent.0,
                            child: parent.1,
                        });
                    }
                }
                if entry.on_stack > 0 {
                    // this is to pick the stack entry (left value)
                    current_path.push(&arena, ChildPos::Left);

                    // now step down the stack until we find the element
                    // the stack grows downwards (indices going up). Now we're starting from
                    // the top of the stack, walking down. So we start at the highest index
                    let stack_pos = self
                        .stack
                        .iter()
                        .rev()
                        .position(|v| *v == idx)
                        .expect("(internal error) node not on stack")
                        as i32;

                    partial_paths.push(PartialPath {
                        path: current_path,
                        stack_pos,
                        idx: 0,
                        child: ChildPos::Left,
                    });
                }
            }
            if used_p {
                cursor += 1;
            } else {
                partial_paths.swap_remove(cursor);
            }
            if cursor >= partial_paths.len() {
                cursor = 0;
                current_length += 1;
            }
        };

        // if this path is too long, we can't return it
        let backref_len = ret.serialized_length();
        // we always need the 0xfe introducer for a back-reference as well, so
        // include that in the serialized size of the path
        if u64::from(backref_len) + 1 > entry.serialized_length {
            None
        } else {
            Some(ret.done())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn test_basic_tree() {
        let mut a = Allocator::new();
        // build this test tree:
        //          r
        //        /     \
        //     b          c
        //   /  \        / \
        //  0  "foobar" 0 "foobar"
        let foo1 = a.new_atom(b"foobar").unwrap();
        let b = a.new_pair(NodePtr::NIL, foo1).unwrap();
        let foo2 = a.new_atom(b"foobar").unwrap();
        let c = a.new_pair(NodePtr::NIL, foo2).unwrap();
        let r = a.new_pair(b, c).unwrap();

        let mut tree = TreeCache::new(None);
        tree.update(&a, r);

        // before we start pushing anything onto the "parse stack" we shouldn't
        // be able to find a path to any node
        for node in &[r, b, c, foo1, foo2, NodePtr::NIL] {
            assert_eq!(tree.find_path(*node), None);
        }

        // trees are built from the bottom up, left to right
        tree.push(NodePtr::NIL);

        // NIL is a special case, we never form a path to it, but it's also too
        // short to form a path to
        for node in &[r, b, c, foo1, foo2, NodePtr::NIL] {
            assert_eq!(tree.find_path(*node), None);
        }

        tree.push(foo1);

        for node in &[r, b, c, NodePtr::NIL] {
            assert_eq!(tree.find_path(*node), None);
        }

        // at this point we should be able to form a path to "foobar", both
        // copies of it. This atom is on the top of the stack
        assert_eq!(tree.find_path(foo1), Some(vec![0b10]));
        assert_eq!(tree.find_path(foo2), Some(vec![0b10]));

        tree.pop2_and_cons(b);

        for node in &[r, NodePtr::NIL] {
            assert_eq!(tree.find_path(*node), None);
        }

        // at this point we should also be able to form a path to b, the
        // subtree. It is now at the top of the stack, and "foobar" is not.
        // since b and c are identical, we can find a path to c as well
        assert_eq!(tree.find_path(b), Some(vec![0b10]));
        assert_eq!(tree.find_path(c), Some(vec![0b10]));
        // "foobar is found as the right node of b, which is at the top of the
        // stack
        assert_eq!(tree.find_path(foo1), Some(vec![0b110]));
        assert_eq!(tree.find_path(foo2), Some(vec![0b110]));

        // now do the right sub-tree
        tree.push(NodePtr::NIL);
        tree.push(foo2);
        tree.pop2_and_cons(c);

        // this subtree is identical to the left hand side, so the "foobar" paths
        // can now point into it

        assert_eq!(tree.find_path(b), Some(vec![0b10]));
        assert_eq!(tree.find_path(c), Some(vec![0b10]));
        // "foobar is found as the right node of c, which is at the top of the
        // stack
        assert_eq!(tree.find_path(foo1), Some(vec![0b110]));
        assert_eq!(tree.find_path(foo2), Some(vec![0b110]));

        tree.pop2_and_cons(r);

        // at this point the complete tree is on the parse stack, and we can
        // find paths to all nodes
        assert_eq!(tree.find_path(c), tree.find_path(b));
        assert!([vec![0b100], vec![0b110]].contains(&tree.find_path(b).unwrap()));
        // "foobar is found as the right node of c and b, which are both left
        // and right side of the root. These paths are equally long and so which
        // one we find doesn't really matter
        assert_eq!(tree.find_path(foo1), tree.find_path(foo2));
        assert!([vec![0b1100], vec![0b1110]].contains(&tree.find_path(foo1).unwrap()));
    }

    #[rstest]
    #[case(0, Some(vec![0b10]))]
    #[case(1, Some(vec![0b100]))]
    #[case(2, Some(vec![0b1000]))]
    #[case(3, Some(vec![0b10000]))]
    #[case(6, Some(vec![0b10000000]))]
    #[case(7, Some(vec![0b1, 0]))]
    #[case(8, Some(vec![0b10, 0]))]
    #[case(14, Some(vec![0b10000000, 0]))]
    #[case(15, Some(vec![0b1, 0, 0]))]
    #[case(22, Some(vec![0b10000000, 0, 0]))]
    #[case(23, Some(vec![0b1, 0, 0, 0]))]
    #[case(30, Some(vec![0b10000000, 0, 0, 0]))]
    #[case(31, Some(vec![0b1, 0, 0, 0, 0]))]
    #[case(36, Some(vec![0b100000, 0, 0, 0, 0]))]
    #[case(37, Some(vec![0b1000000, 0, 0, 0, 0]))]
    #[case(38, Some(vec![0b10000000, 0, 0, 0, 0]))]
    // at this point the path is longer than the atom we're referencing
    #[case(39, None)]
    #[case(40, None)]
    #[case(400, None)]
    fn test_deep_tree(#[case] n: u32, #[case] expect: Option<Vec<u8>>) {
        let mut a = Allocator::new();

        let foo = a.new_atom(b"foobar").unwrap();
        let mut links = vec![foo];
        for _i in 0..n {
            let node = a.new_pair(*links.last().unwrap(), NodePtr::NIL).unwrap();
            links.push(node);
        }

        let root = *links.last().unwrap();
        let mut tree = TreeCache::new(None);
        tree.update(&a, root);

        tree.push(foo);
        for link in &links[1..] {
            tree.push(NodePtr::NIL);
            tree.pop2_and_cons(*link);
        }

        assert_eq!(tree.find_path(foo), expect);
    }

    #[rstest]
    #[case(0, Some(vec![0b10]))]
    #[case(1, Some(vec![0b101]))]
    #[case(2, Some(vec![0b1011]))]
    #[case(3, Some(vec![0b10111]))]
    #[case(6, Some(vec![0b10111111]))]
    #[case(7, Some(vec![0b1, 0b01111111]))]
    #[case(8, Some(vec![0b10, 0xff]))]
    #[case(14, Some(vec![0b10111111, 0xff]))]
    #[case(15, Some(vec![0b1, 0b01111111, 0xff]))]
    #[case(22, Some(vec![0b10111111, 0xff, 0xff]))]
    #[case(23, Some(vec![0b1, 0b01111111, 0xff, 0xff]))]
    #[case(30, Some(vec![0b10111111, 0xff, 0xff, 0xff]))]
    #[case(31, Some(vec![0b1, 0b01111111, 0xff, 0xff, 0xff]))]
    #[case(36, Some(vec![0b101111, 0xff, 0xff, 0xff, 0xff]))]
    #[case(37, Some(vec![0b1011111, 0xff, 0xff, 0xff, 0xff]))]
    #[case(38, Some(vec![0b10111111, 0xff, 0xff, 0xff, 0xff]))]
    // at this point the path is longer than the atom we're referencing
    #[case(39, None)]
    #[case(40, None)]
    #[case(400, None)]
    fn test_deep_stack(#[case] n: u32, #[case] expect: Option<Vec<u8>>) {
        let mut a = Allocator::new();

        let foo = a.new_atom(b"foobar").unwrap();
        let mut links = vec![foo];
        for _i in 0..n {
            let node = a.new_pair(*links.last().unwrap(), NodePtr::NIL).unwrap();
            links.push(node);
        }

        let root = *links.last().unwrap();
        let mut tree = TreeCache::new(None);
        tree.update(&a, root);

        tree.push(foo);
        for _link in &links[1..] {
            tree.push(NodePtr::NIL);
            // we don't pop anything here, so these nodes are all on the parse
            // stack, which means we traverse them along the right nodes
        }

        assert_eq!(tree.find_path(foo), expect);
    }

    #[rstest]
    #[case(0, Some(vec![0b10]))]
    #[case(1, Some(vec![0b100]))]
    #[case(2, Some(vec![0b1000]))]
    #[case(3, Some(vec![0b10000]))]
    #[case(6, Some(vec![0b10000000]))]
    #[case(7, Some(vec![0b1, 0]))]
    #[case(8, Some(vec![0b10, 0]))]
    #[case(14, Some(vec![0b10000000, 0]))]
    #[case(15, Some(vec![0b1, 0, 0]))]
    #[case(22, Some(vec![0b10000000, 0, 0]))]
    // at this point the path is longer than the atom we're referencing
    #[case(23, None)]
    #[case(30, None)]
    #[case(31, None)]
    #[case(36, None)]
    #[case(37, None)]
    #[case(38, None)]
    #[case(39, None)]
    #[case(40, None)]
    #[case(400, None)]
    fn test_single_byte(#[case] n: u32, #[case] expect: Option<Vec<u8>>) {
        let mut a = Allocator::new();

        // this is the shortest atom we form back references to
        let foo = a.new_atom(b"fooo").unwrap();
        let mut links = vec![foo];
        for _i in 0..n {
            let node = a.new_pair(*links.last().unwrap(), NodePtr::NIL).unwrap();
            links.push(node);
        }

        let root = *links.last().unwrap();
        let mut tree = TreeCache::new(None);
        tree.update(&a, root);

        tree.push(foo);
        for link in &links[1..] {
            tree.push(NodePtr::NIL);
            tree.pop2_and_cons(*link);
        }

        assert_eq!(tree.find_path(foo), expect);
    }
}
