use bitflags::Flags;
use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::reduction::Reduction;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::{ChiaDialect, ClvmFlags, run_program_with_counters};
use std::time::Instant;

/// Run a hex-encoded CLVM program and print execution stats
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to file containing hex-encoded CLVM program
    filename: String,

    /// Arguments to pass to the program as a list. Integers are created as
    /// number atoms, other strings are used as raw byte atoms.
    #[arg(long, num_args = 1.., conflicts_with = "envfile")]
    env: Vec<String>,

    /// Path to file containing hex-encoded CLVM environment
    #[arg(long, conflicts_with = "env")]
    envfile: Option<String>,

    /// CLVM dialect flags to enable
    #[arg(long, num_args = 1..)]
    flags: Vec<String>,
}

pub fn main() {
    let args = Args::parse();

    let hex_str = std::fs::read_to_string(&args.filename)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", args.filename));
    let program_bytes = hex::decode(hex_str.trim()).expect("invalid hex");

    let mut a = Allocator::new();
    let program = node_from_bytes_backrefs(&mut a, &program_bytes).expect("invalid CLVM");

    let env = if let Some(envfile) = &args.envfile {
        let env_hex = std::fs::read_to_string(envfile)
            .unwrap_or_else(|e| panic!("failed to read {envfile}: {e}"));
        let env_bytes = hex::decode(env_hex.trim()).expect("invalid hex in envfile");
        node_from_bytes_backrefs(&mut a, &env_bytes).expect("invalid CLVM in envfile")
    } else {
        let mut env = NodePtr::NIL;
        for val in args.env.into_iter().rev() {
            let atom = if let Ok(num) = val.parse::<i64>() {
                a.new_number(num.into()).expect("new_number")
            } else {
                a.new_atom(val.as_bytes()).expect("new_atom")
            };
            env = a.new_pair(atom, env).expect("new_pair");
        }
        env
    };

    let mut flags = ClvmFlags::empty();
    for f in &args.flags {
        let matched = ClvmFlags::FLAGS
            .iter()
            .find(|flag| flag.name() == f.as_str());
        match matched {
            Some(flag) => flags |= *flag.value(),
            None => {
                let valid: Vec<&str> = ClvmFlags::FLAGS.iter().map(|f| f.name()).collect();
                panic!("unknown flag: {f}. Valid flags: {}", valid.join(", "));
            }
        }
    }
    let dialect = ChiaDialect::new(flags);
    let max_cost: u64 = 11_000_000_000;

    let start = Instant::now();
    let (counters, result) = run_program_with_counters(&mut a, &dialect, program, env, max_cost);
    let duration = start.elapsed();

    match result {
        Ok(Reduction(cost, _result)) => {
            println!("cost: {cost}");
            println!("execution time: {duration:.3?}");
            let ns_per_cost = duration.as_nanos() as f64 / cost as f64;
            println!("ns/cost: {ns_per_cost:.3}");
        }
        Err(e) => {
            println!("execution FAILED: {e:?}");
            println!("execution time: {duration:.3?}");
        }
    }
    println!("atom_count: {}", counters.atom_count);
    println!("pair_count: {}", counters.pair_count);
    println!("heap_size: {}", counters.heap_size);
    println!("max_atom_count: {}", counters.max_atom_count);
    println!("max_pair_count: {}", counters.max_pair_count);
    println!("max_heap_size: {}", counters.max_heap_size);
    println!("val_stack_usage: {}", counters.val_stack_usage);
    println!("env_stack_usage: {}", counters.env_stack_usage);
    println!("op_stack_usage: {}", counters.op_stack_usage);
    println!("allocated_atom_count: {}", a.allocated_atom_count());
    println!("allocated_pair_count: {}", a.allocated_pair_count());
    println!("allocated_heap_size: {}", a.allocated_heap_size());
}
