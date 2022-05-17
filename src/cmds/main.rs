use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::serialization_cache::{serialized_length, treehash, ObjectCache};
use clvmr::serialize::{node_from_bytes, CONS_BOX_MARKER};
use hex::{decode, encode};
//use std::env::args;
use clvmr::stack_cache::StackCache;

#[derive(Parser)]
struct Cli {
    // #[clap(parse(from_hex))]
    input_program_string: String,
}

#[derive(PartialEq, Eq)]
enum ReadOp {
    Parse,
    Cons,
}

const BACK_REFERENCE: u8 = 0xFE;

fn append_atom_encoding_prefix(v: &mut Vec<u8>, atom: &[u8]) {
    let size = atom.len();
    if size == 0 {
        v.push(0x80);
        return;
    }

    if size == 1 && atom[0] < 0x80 {
        return;
    }

    if size < 0x40 {
        v.push(0x80 | (size as u8));
    } else if size < 0x2000 {
        v.push(0xc0 | (size >> 8) as u8);
        v.push(size as u8);
    } else if size < 0x100000 {
        v.push(0xe0 | (size >> 15) as u8);
        v.push((size >> 8) as u8);
        v.push(size as u8);
    } else if size < 0x8000000 {
        v.push(0xf0 | (size >> 22) as u8);
        v.push((size >> 16) as u8);
        v.push((size >> 8) as u8);
        v.push((size) as u8);
    } else {
        dbg!(size);
        todo!();
    }
}

fn push_encoded_atom(r: &mut Vec<u8>, atom: &[u8]) {
    append_atom_encoding_prefix(r, atom);
    r.extend_from_slice(atom);
}

fn sexp_to_u8_v2(allocator: &Allocator, node: NodePtr) -> Vec<u8> {
    let mut r = vec![];
    let mut read_op_stack: Vec<ReadOp> = vec![ReadOp::Parse];
    let mut write_stack: Vec<NodePtr> = vec![node];

    let mut stack_cache = StackCache::new();

    let mut thc = ObjectCache::new(allocator, treehash);
    let mut slc = ObjectCache::new(allocator, serialized_length);

    while !write_stack.is_empty() {
        let node_to_write = write_stack.pop().expect("write_stack empty");

        let op = read_op_stack.pop();
        assert!(op == Some(ReadOp::Parse));

        let node_serialized_length = *slc
            .get(&node_to_write)
            .expect("couldn't calculate serialized length");
        let node_tree_hash = thc.get(&node_to_write).expect("can't get treehash");
        match stack_cache.find_path(node_tree_hash, node_serialized_length) {
            Some(path) => {
                r.push(BACK_REFERENCE);
                push_encoded_atom(&mut r, &path);
                stack_cache.push(node_tree_hash.clone());
            }
            None => match allocator.sexp(node_to_write) {
                SExp::Pair(left, right) => {
                    r.push(CONS_BOX_MARKER);
                    write_stack.push(right);
                    write_stack.push(left);
                    read_op_stack.push(ReadOp::Cons);
                    read_op_stack.push(ReadOp::Parse);
                    read_op_stack.push(ReadOp::Parse);
                }
                SExp::Atom(atom_buf) => {
                    let atom = allocator.buf(&atom_buf);
                    push_encoded_atom(&mut r, atom);
                    stack_cache.push(node_tree_hash.clone());
                }
            },
        }
        while !read_op_stack.is_empty() && read_op_stack[read_op_stack.len() - 1] == ReadOp::Cons {
            read_op_stack.pop();
            stack_cache.pop2_and_cons();
        }
    }
    r
}

fn main() {
    let args = Cli::parse();
    let input_program = decode(args.input_program_string).expect("can't parse hex");
    let mut allocator = Allocator::new();
    let node = node_from_bytes(&mut allocator, &input_program).expect("can't deserialize");
    //let mut thc = ObjectCache::new(&allocator, treehash);
    //println!("{:?}", encode(thc.get(&node).unwrap()));
    //println!("{:?}", thc.invert());
    //let mut slc = ObjectCache::new(&allocator, serialized_length);
    //println!("{:?}", slc.get(&node).unwrap());
    let t = sexp_to_u8_v2(&allocator, node);
    println!("{:?}", encode(t));
    //let mut pc = ObjectCache::new(&allocator, parent_path);
    //println!("{:?}", pc.get(&node).unwrap());
}
