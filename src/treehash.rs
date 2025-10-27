use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::ObjectType;
use crate::SExp;
use chia_sha2::Sha256;
use hex_literal::hex;
use std::fmt;
use std::ops::Deref;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreeHash([u8; 32]);

impl TreeHash {
    pub const fn new(hash: [u8; 32]) -> Self {
        Self(hash)
    }

    pub const fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl fmt::Debug for TreeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TreeHash({self})")
    }
}

impl fmt::Display for TreeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl From<[u8; 32]> for TreeHash {
    fn from(hash: [u8; 32]) -> Self {
        Self::new(hash)
    }
}

impl From<TreeHash> for [u8; 32] {
    fn from(hash: TreeHash) -> [u8; 32] {
        hash.0
    }
}

impl AsRef<[u8]> for TreeHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for TreeHash {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn tree_hash_atom(bytes: &[u8]) -> TreeHash {
    let mut sha256 = Sha256::new();
    sha256.update([1]);
    sha256.update(bytes);
    TreeHash::new(sha256.finalize())
}

pub fn tree_hash_pair(first: TreeHash, rest: TreeHash) -> TreeHash {
    let mut sha256 = Sha256::new();
    sha256.update([2]);
    sha256.update(first);
    sha256.update(rest);
    TreeHash::new(sha256.finalize())
}

#[derive(Default)]
pub struct TreeCache {
    hashes: Vec<TreeHash>,
    // parallel vector holding the cost used to compute the corresponding hash
    costs: Vec<Cost>,
    // each entry is an index into hashes and costs, or one of 3 special values:
    // u16::MAX if the pair has not been visited
    // u16::MAX - 1 if the pair has been seen once
    // u16::MAX - 2 if the pair has been seen at least twice (this makes it a
    // candidate for memoization)
    pairs: Vec<u16>,
}

const NOT_VISITED: u16 = u16::MAX;
const SEEN_ONCE: u16 = u16::MAX - 1;
const SEEN_MULTIPLE: u16 = u16::MAX - 2;

impl TreeCache {
    /// Get cached hash and its associated cost (if present).
    pub fn get(&self, n: NodePtr) -> Option<(&TreeHash, Cost)> {
        // We only cache pairs (for now)
        if !matches!(n.object_type(), ObjectType::Pair) {
            return None;
        }

        let idx = n.index() as usize;
        let slot = *self.pairs.get(idx)?;
        if slot >= SEEN_MULTIPLE {
            return None;
        }
        Some((&self.hashes[slot as usize], self.costs[slot as usize]))
    }

    /// Insert a cached hash with its associated cost. If the cache is full we
    /// ignore the insertion.
    pub fn insert(&mut self, n: NodePtr, hash: &TreeHash, cost: Cost) {
        // If we've reached the max size, just ignore new cache items
        if self.hashes.len() == SEEN_MULTIPLE as usize {
            return;
        }

        if !matches!(n.object_type(), ObjectType::Pair) {
            return;
        }

        let idx = n.index() as usize;
        if idx >= self.pairs.len() {
            self.pairs.resize(idx + 1, NOT_VISITED);
        }

        let slot = self.hashes.len();
        self.hashes.push(*hash);
        self.costs.push(cost);
        self.pairs[idx] = slot as u16;
    }

    /// mark the node as being visited. Returns true if we need to
    /// traverse visitation down this node.
    fn visit(&mut self, n: NodePtr) -> bool {
        if !matches!(n.object_type(), ObjectType::Pair) {
            return false;
        }
        let idx = n.index() as usize;
        if idx >= self.pairs.len() {
            self.pairs.resize(idx + 1, NOT_VISITED);
        }
        if self.pairs[idx] > SEEN_MULTIPLE {
            self.pairs[idx] -= 1;
        }
        self.pairs[idx] == SEEN_ONCE
    }

    pub fn should_memoize(&mut self, n: NodePtr) -> bool {
        if !matches!(n.object_type(), ObjectType::Pair) {
            return false;
        }
        let idx = n.index() as usize;
        if idx >= self.pairs.len() {
            false
        } else {
            self.pairs[idx] <= SEEN_MULTIPLE
        }
    }

    pub fn visit_tree(&mut self, a: &Allocator, node: NodePtr) {
        if !self.visit(node) {
            return;
        }
        let mut nodes = vec![node];
        while let Some(n) = nodes.pop() {
            let SExp::Pair(left, right) = a.sexp(n) else {
                continue;
            };
            if self.visit(left) {
                nodes.push(left);
            }
            if self.visit(right) {
                nodes.push(right);
            }
        }
    }
}

pub(crate) enum TreeOp {
    SExp(NodePtr),
    Cons,
    ConsAddCacheCost(NodePtr, Cost),
}

macro_rules! th {
    ($hash:expr) => {
        TreeHash::new(hex!($hash))
    };
}
pub const PRECOMPUTED_HASHES: [TreeHash; 24] = [
    th!("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
    th!("9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2"),
    th!("a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222"),
    th!("c79b932e1e1da3c0e098e5ad2c422937eb904a76cf61d83975a74a68fbb04b99"),
    th!("a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5"),
    th!("bc5959f43bc6e47175374b6716e53c9a7d72c59424c821336995bad760d9aeb3"),
    th!("44602a999abbebedf7de0ae1318e4f57e3cb1d67e482a65f9657f7541f3fe4bb"),
    th!("ca6c6588fa01171b200740344d354e8548b7470061fb32a34f4feee470ec281f"),
    th!("9e6282e4f25e370ce617e21d6fe265e88b9e7b8682cf00059b9d128d9381f09d"),
    th!("ac9e61d54eb6967e212c06aab15408292f8558c48f06f9d705150063c68753b0"),
    th!("c04b5bb1a5b2eb3e9cd4805420dba5a9d133da5b7adeeafb5474c4adae9faa80"),
    th!("57bfd1cb0adda3d94315053fda723f2028320faa8338225d99f629e3d46d43a9"),
    th!("6b6daa8334bbcc8f6b5906b6c04be041d92700b74024f73f50e0a9f0dae5f06f"),
    th!("c7b89cfb9abf2c4cb212a4840b37d762f4c880b8517b0dadb0c310ded24dd86d"),
    th!("653b3bb3e18ef84d5b1e8ff9884aecf1950c7a1c98715411c22b987663b86dda"),
    th!("24255ef5d941493b9978f3aabb0ed07d084ade196d23f463ff058954cbf6e9b6"),
    th!("af340aa58ea7d72c2f9a7405f3734167bb27dd2a520d216addef65f8362102b6"),
    th!("26e7f98cfafee5b213726e22632923bf31bf3e988233235f8f5ca5466b3ac0ed"),
    th!("115b498ce94335826baa16386cd1e2fde8ca408f6f50f3785964f263cdf37ebe"),
    th!("d8c50d6282a1ba47f0a23430d177bbfbb72e2b84713745e894f575570f1f3d6e"),
    th!("dbe726e81a7221a385e007ef9e834a975a4b528c6f55a5d2ece288bee831a3d1"),
    th!("764c8a3561c7cf261771b4e1969b84c210836f3c034baebac5e49a394a6ee0a9"),
    th!("dce37f3512b6337d27290436ba9289e2fd6c775494c33668dd177cf811fbd47a"),
    th!("5809addc9f6926fc5c4e20cf87958858c4454c21cdfc6b02f377f12c06b35cca"),
];
