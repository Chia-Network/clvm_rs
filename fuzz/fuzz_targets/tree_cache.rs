#![no_main]
mod make_tree;
mod node_eq;
mod serialized_len;

use clvmr::reduction::Reduction;
use clvmr::serde::TreeCache;
use clvmr::traverse_path::traverse_path;
use clvmr::{Allocator, NodePtr, SExp};
use libfuzzer_sys::fuzz_target;
use make_tree::make_tree_limits;
use node_eq::node_eq;
use serialized_len::compute_serialized_len;

#[derive(PartialEq, Eq)]
enum ReadOp {
    Parse,
    Cons(NodePtr),
}

fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let (tree, node_count) = make_tree_limits(&mut allocator, &mut unstructured, 1000, true);
    // uncomment this if you find an interesting test case to add to the benchmark
    /*
        let tmp = clvmr::serde::node_to_bytes_backrefs(&allocator, tree).unwrap();
        std::fs::write("serialized-benchmark.generator", &tmp).expect("fs::write()");
    */
    let mut tree_cache = TreeCache::default();
    tree_cache.update(&allocator, tree);

    let mut read_op_stack = vec![ReadOp::Parse];
    let mut write_stack = vec![tree];

    // we count down until this hits zero, then we know which node to test
    let mut node_idx = unstructured.int_in_range(0..=node_count).unwrap_or(5) as i32;
    let mut node_to_test: Option<(NodePtr, usize)> = None;

    // the stack, as it's built from the parser's point of view. This is what
    // the back-references make lookups into.
    let mut parse_stack = NodePtr::NIL;

    let mut stack_depth = 0;
    while let Some(node_to_write) = write_stack.pop() {
        let op = read_op_stack.pop();
        assert!(op == Some(ReadOp::Parse));

        // make sure we find a valid path to the node we're testing
        // This is the main test of the fuzzer
        if let Some((node, serialized_len)) = node_to_test {
            if let Some(path) = tree_cache.find_path(node) {
                let Ok(Reduction(_, found_node)) = traverse_path(&allocator, &path, parse_stack)
                else {
                    println!("invalid path {path:?} parse stack:");
                    let mut s = parse_stack;
                    while let SExp::Pair(item, next) = allocator.sexp(s) {
                        println!("  {item:?}");
                        s = next;
                    }
                    panic!("failed");
                };
                // make sure the path we returned actually points to an atom
                // that's equivalent
                if !node_eq(&allocator, found_node, node) {
                    println!("path: {:?}", path);
                    println!("found: {found_node:?} expected: {node:?}");
                    panic!("failed");
                }
                if serialized_len <= path.len() {
                    println!(
                        "node serialized size: {serialized_len} backref size: {}",
                        path.len() + 1
                    );
                }
                assert!(serialized_len > path.len() + 1);
            }
        }

        match tree_cache.find_path(node_to_write) {
            Some(_path) => {
                tree_cache.push(node_to_write);
                parse_stack = allocator.new_pair(node_to_write, parse_stack).unwrap();
                stack_depth += 1;
            }
            None => match allocator.sexp(node_to_write) {
                SExp::Pair(left, right) => {
                    write_stack.push(right);
                    write_stack.push(left);
                    read_op_stack.push(ReadOp::Cons(node_to_write));
                    read_op_stack.push(ReadOp::Parse);
                    read_op_stack.push(ReadOp::Parse);
                }
                SExp::Atom => {
                    tree_cache.push(node_to_write);
                    parse_stack = allocator.new_pair(node_to_write, parse_stack).unwrap();
                    stack_depth += 1;
                    if node_idx == 0 {
                        let serialized_len =
                            compute_serialized_len(&allocator, node_to_write) as usize;
                        node_to_test = Some((node_to_write, serialized_len));
                    }
                    node_idx -= 1;
                }
            },
        }
        while let Some(ReadOp::Cons(node)) = read_op_stack.last() {
            let node = *node;
            read_op_stack.pop();
            tree_cache.pop2_and_cons(node);
            if node_idx == 0 {
                let serialized_len = compute_serialized_len(&allocator, node_to_write) as usize;
                node_to_test = Some((node_to_write, serialized_len));
            }
            node_idx -= 1;

            let SExp::Pair(right, rest) = allocator.sexp(parse_stack) else {
                panic!("internal error");
            };
            let SExp::Pair(left, rest) = allocator.sexp(rest) else {
                panic!("internal error");
            };
            let new_root = allocator.new_pair(left, right).unwrap();
            parse_stack = allocator.new_pair(new_root, rest).unwrap();
            stack_depth -= 1;
        }
    }
    assert_eq!(stack_depth, 1);
});
