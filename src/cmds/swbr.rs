use clap::Parser;
use clvmr::compressor::reserialize;
use hex::{decode, encode};

#[derive(Parser)]
struct Cli {
    #[clap(parse(try_from_str = decode))]
    input_program: Vec<Vec<u8>>,

    #[clap(short, long)]
    uncompressed_output: bool,

    #[clap(short, long)]
    include_deserialize_program: bool,
}

fn main() {
    let args = Cli::parse();

    let blob = reserialize(&args.input_program[0], args.include_deserialize_program)
        .expect("bad serialization");

    let output = encode(blob);
    println!("{}", output);
}
