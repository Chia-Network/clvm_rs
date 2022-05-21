use clap::Parser;
use clvmr::allocator::Allocator;
use clvmr::node::Node;
use clvmr::serialize::{node_from_bytes, node_to_bytes_backrefs};
use hex::{decode, encode};
//use std::env::args;

#[derive(Parser)]
struct Cli {
    // #[clap(parse(from_hex))]
    input_program_string: String,

    #[clap(short, long, parse(from_occurrences))]
    allow_backreferences: usize,
}

fn main() {
    let args = Cli::parse();
    let input_program = decode(args.input_program_string).expect("can't parse hex");
    let mut allocator = Allocator::new();
    let node_ptr = node_from_bytes(&mut allocator, &input_program).expect("can't deserialize");
    //let mut thc = ObjectCache::new(&allocator, treehash);
    //println!("{:?}", encode(thc.get(&node).unwrap()));
    //println!("{:?}", thc.invert());
    //let mut slc = ObjectCache::new(&allocator, serialized_length);
    //println!("{:?}", slc.get(&node).unwrap());
    let t = node_to_bytes_backrefs(&Node::new(&allocator, node_ptr)).unwrap();
    println!("{:?}", encode(t));
    //let mut pc = ObjectCache::new(&allocator, parent_path);
    //println!("{:?}", pc.get(&node).unwrap());
}
