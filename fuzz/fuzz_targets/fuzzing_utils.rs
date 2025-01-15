use chia_sha2::Sha256;
use clvmr::allocator::{Allocator, NodePtr, SExp};

#[allow(dead_code)]
fn hash_atom(buf: &[u8]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update([1_u8]);
    ctx.update(buf);
    ctx.finalize()
}

#[allow(dead_code)]
fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update([2_u8]);
    ctx.update(left);
    ctx.update(right);
    ctx.finalize()
}

#[allow(dead_code)]
enum TreeOp {
    SExp(NodePtr),
    Cons,
}

#[allow(dead_code)]
pub fn tree_hash(a: &Allocator, node: NodePtr) -> [u8; 32] {
    let mut hashes = Vec::new();
    let mut ops = vec![TreeOp::SExp(node)];

    while let Some(op) = ops.pop() {
        match op {
            TreeOp::SExp(node) => match a.sexp(node) {
                SExp::Atom => {
                    hashes.push(hash_atom(a.atom(node).as_ref()));
                }
                SExp::Pair(left, right) => {
                    ops.push(TreeOp::Cons);
                    ops.push(TreeOp::SExp(left));
                    ops.push(TreeOp::SExp(right));
                }
            },
            TreeOp::Cons => {
                let first = hashes.pop().unwrap();
                let rest = hashes.pop().unwrap();
                hashes.push(hash_pair(&first, &rest));
            }
        }
    }

    assert!(hashes.len() == 1);
    hashes[0]
}

#[allow(dead_code)]
pub fn visit_tree(a: &Allocator, node: NodePtr, mut visit: impl FnMut(&Allocator, NodePtr)) {
    let mut nodes = vec![node];
    let mut visited_index = 0;

    while nodes.len() > visited_index {
        match a.sexp(nodes[visited_index]) {
            SExp::Atom => {}
            SExp::Pair(left, right) => {
                nodes.push(left);
                nodes.push(right);
            }
        }
        visited_index += 1;
    }

    // visit nodes bottom-up (right to left).
    for node in nodes.into_iter().rev() {
        visit(a, node);
    }
}
