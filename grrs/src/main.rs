use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::serialization_cache::{serialized_length, treehash, ObjectCache};
use clvmr::serialize::{node_from_bytes, CONS_BOX_MARKER};
use hex::{decode, encode};
//use std::env::args;
use std::collections::HashMap;

#[derive(Parser)]
struct Cli {
    // #[clap(parse(from_hex))]
    input_program_string: String,
}

fn pop_stack(
    allocator: &Allocator,
    stack: NodePtr,
    stack_usage_count: &mut HashMap<NodePtr, usize>,
    stack_node_parents: &mut HashMap<NodePtr, Vec<NodePtr>>,
) -> (NodePtr, NodePtr) {
    (0, 0)
}

fn push_stack(
    allocator: &Allocator,
    new_pair: NodePtr,
    stack: NodePtr,
    stack_usage_count: &mut HashMap<NodePtr, usize>,
    stack_node_parents: &mut HashMap<NodePtr, Vec<NodePtr>>,
) -> NodePtr {
    0
}

fn path_in_stack(
    sexp: NodePtr,
    read_stack: NodePtr,
    stack_usage_count: &HashMap<NodePtr, usize>,
    stack_node_parents: &HashMap<NodePtr, Vec<NodePtr>>,
    node_serialized_length: usize,
) -> Option<Vec<u8>> {
    None
}

fn append_atom_encoding_prefix(v: &mut Vec<u8>, atom: &[u8]) -> () {
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
        v.push((size & 0xff) as u8);
    } else if size < 0x100000 {
        v.push(0xe0 | (size >> 15) as u8);
        v.push((size >> 8) as u8 & 0xff);
        v.push((size & 0xff) as u8);
    } else if size < 0x8000000 {
        v.push(0xf0 | (size >> 22) as u8);
        v.push((size >> 16) as u8 & 0xff);
        v.push((size >> 8) as u8 & 0xff);
        v.push((size & 0xff) as u8);
    }
    todo!();
}

fn sexp_to_u8_v2(allocator: &Allocator, node: NodePtr) -> Vec<u8> {
    let mut r = vec![];
    let mut read_op_stack: Vec<u8> = vec![0]; // 0 = "parse", 1 = "cons"
    let mut write_stack: Vec<NodePtr> = vec![node];

    let mut read_stack: NodePtr = allocator.null();
    let mut stack_usage_count: HashMap<NodePtr, usize> = HashMap::new();
    let mut stack_node_parents: HashMap<NodePtr, Vec<NodePtr>> = HashMap::new();

    let mut thc = ObjectCache::new(allocator, treehash);
    let mut slc = ObjectCache::new(allocator, serialized_length);

    while write_stack.len() > 0 {
        while read_op_stack[read_op_stack.len() - 1] == 1 {
            read_op_stack.pop();
            let (right, rs) = pop_stack(
                allocator,
                read_stack,
                &mut stack_usage_count,
                &mut stack_node_parents,
            );
            let (left, rs) = pop_stack(
                allocator,
                rs,
                &mut stack_usage_count,
                &mut stack_node_parents,
            );
            /*
            let new_pair = allocator.new_pair(left, right).expect("out of memory");
            read_stack = push_stack(
                allocator,
                new_pair,
                read_stack,
                &mut stack_usage_count,
                &mut stack_node_parents,
            );
            */
        }

        let node_to_write = write_stack.pop().expect("write_stack empty");

        let op = read_op_stack.pop();
        assert!(op == Some(0));

        let node_serialized_length = slc
            .get(&node_to_write)
            .expect("couldn't calculate serialized length");
        match path_in_stack(
            node_to_write,
            read_stack,
            &stack_usage_count,
            &stack_node_parents,
            *node_serialized_length,
        ) {
            Some(path) => {}
            None => match allocator.sexp(node_to_write) {
                SExp::Pair(left, right) => {
                    r.push(CONS_BOX_MARKER);
                    write_stack.push(right);
                    write_stack.push(left);
                    read_op_stack.push(1);
                    read_op_stack.push(0);
                    read_op_stack.push(0);
                }
                SExp::Atom(atom_buf) => {
                    let atom = allocator.buf(&atom_buf);
                    append_atom_encoding_prefix(&mut r, atom);
                    r.extend_from_slice(atom);
                }
            },
        }

        /*
          from .SExp import SExp

          CLVMObject = SExp.to
          tag_with_tree_hash(sexp)
          write_stack = [sexp]
          read_stack = CLVMObject(b"")
          tag_with_tree_hash(read_stack)

          read_stack_memo = {}

          read_op_stack = ["P"]

          while write_stack:

              check_stack_vs_memo(read_stack, read_stack_memo)

              while read_op_stack[-1] == "C":
                  read_op_stack.pop()
                  right, read_stack = pop_stack(read_stack)
                  left, read_stack = pop_stack(read_stack)
                  new_obj = create_pair(left, right, read_stack_memo)
                  read_stack = push_stack(new_obj, read_stack, read_stack_memo)
                  check_stack_vs_memo(read_stack, read_stack_memo)

              sexp = write_stack.pop()
              pair = sexp.pair

              path = path_in_stack(sexp, read_stack, read_stack_memo, sexp.serialized_length)
              if path is not None:
                  yield bytes([BACK_REFERENCE])
                  yield from atom_to_byte_iterator(path)

                  # TODO: read object from path and ensure it's correct

                  read_stack = push_stack(sexp, read_stack, read_stack_memo)

                  op = read_op_stack.pop()
                  assert op == "P"
                  continue
              if pair:
                  yield bytes([CONS_BOX_MARKER])
                  write_stack.append(pair[1])
                  write_stack.append(pair[0])

                  op = read_op_stack.pop()
                  assert op == "P"

                  read_op_stack.append("C")
                  read_op_stack.append("P")
                  read_op_stack.append("P")
                  continue
              else:
                  yield from atom_to_byte_iterator(sexp.atom)
                  op = read_op_stack.pop()
                  assert op == "P"
                  read_stack = push_stack(sexp, read_stack, read_stack_memo)

          if any(_ != "C" for _ in read_op_stack):
              breakpoint()

          assert all(_ == "C" for _ in read_op_stack)

        */
    }
    r
}

fn main() {
    let args = Cli::parse();
    let input_program = decode(args.input_program_string).expect("can't parse hex");
    println!("Hello, world!: {:?}", input_program);
    let mut allocator = Allocator::new();
    let node = node_from_bytes(&mut allocator, &input_program).expect("can't deserialize");
    println!("{:?}", node);
    let mut thc = ObjectCache::new(&allocator, treehash);
    println!("{:?}", encode(thc.get(&node).unwrap()));
    let mut slc = ObjectCache::new(&allocator, serialized_length);
    println!("{:?}", slc.get(&node).unwrap());
    let t = sexp_to_u8_v2(&allocator, node);
    println!("{:?}", encode(t));
}
