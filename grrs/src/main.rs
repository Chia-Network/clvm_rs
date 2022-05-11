use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::serialization_cache::{generate_cache, serialized_length, treehash, ObjectCache};
use clvmr::serialize::node_from_bytes;
use hex::{decode, encode};
//use std::env::args;
use std::collections::HashMap;

#[derive(Parser)]
struct Cli {
    // #[clap(parse(from_hex))]
    input_program_string: String,
}

fn sexp_to_byte_iterator_v2(allocator: &mut Allocator, node: NodePtr) -> Vec<u8> {
    let mut read_op_stack: Vec<u8> = vec![0]; // 0 = "parse", 1 = "cons"
    let mut write_stack: Vec<NodePtr> = vec![node];
    let mut stack_usage_count: HashMap<NodePtr, usize> = HashMap::new();
    let mut stack: NodePtr = allocator.null();

    let thc = generate_cache(allocator, node, treehash);
    println!("{:?}", encode(thc.get(&node).unwrap()));
    let slc = generate_cache(allocator, node, serialized_length);

    while write_stack.len() > 0 {}
    vec![]
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
}
