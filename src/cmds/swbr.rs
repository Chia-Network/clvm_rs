use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::compressor::{compress_with_backrefs, decompress};
use clvmr::node::Node;
use clvmr::serialize::{node_to_bytes, node_to_bytes_backrefs};
use hex::{decode, encode};
use std::error::Error;

struct AllocatorNode {
    allocator: Allocator,
    node_ptr: NodePtr,
}

fn node_from_hex(s: &str) -> Result<AllocatorNode, Box<dyn Error + Send + Sync + 'static>> {
    let input_program = decode(s)?;
    let mut allocator = Allocator::new();
    let node_ptr = decompress(&mut allocator, &input_program)?;
    Ok(AllocatorNode {
        allocator,
        node_ptr,
    })
}

#[derive(Parser)]
struct Cli {
    #[clap(parse(try_from_str = node_from_hex))]
    input_program: AllocatorNode,

    #[clap(short, long)]
    uncompressed_output: bool,

    #[clap(short, long)]
    include_deserialize_program: bool,
}

fn main() {
    let args = Cli::parse();

    let mut allocator = args.input_program.allocator;
    let node_ptr = args.input_program.node_ptr;

    let blob = if args.include_deserialize_program {
        compress_with_backrefs(&mut allocator, node_ptr)
    } else if args.uncompressed_output {
        node_to_bytes(&Node::new(&allocator, node_ptr))
    } else {
        node_to_bytes_backrefs(&Node::new(&allocator, node_ptr))
    }
    .expect("bad serialization");

    let output = encode(blob);
    println!("{}", output);
}
