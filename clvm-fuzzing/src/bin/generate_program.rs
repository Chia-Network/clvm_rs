use anyhow::Result;
use arbitrary::Unstructured;
use clap::Parser;
use clvm_fuzzing::{make_clvm_program, make_tree};
use clvmr::allocator::Allocator;
use clvmr::serde::node_to_bytes;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

#[derive(Parser)]
struct Args {
    /// Random seed used to generate program and environment
    seed: u64,
    /// Output file path for serialized program (hex)
    program_out: std::path::PathBuf,
    /// Output file path for serialized environment (hex)
    env_out: std::path::PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut rng = StdRng::seed_from_u64(args.seed);
    let mut data = vec![0u8; 65_536];
    rng.fill_bytes(&mut data);
    let mut unstructured = Unstructured::new(&data);
    let mut allocator = Allocator::new();

    let (env, _env_nodes) = make_tree(&mut allocator, &mut unstructured);
    let program = make_clvm_program(&mut allocator, &mut unstructured, env, 100_000)?;

    let program_hex = hex::encode(node_to_bytes(&allocator, program)?);
    let env_hex = hex::encode(node_to_bytes(&allocator, env)?);

    std::fs::write(&args.program_out, program_hex)?;
    std::fs::write(&args.env_out, env_hex)?;

    Ok(())
}
