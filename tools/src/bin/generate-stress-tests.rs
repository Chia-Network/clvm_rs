use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::reduction::Reduction;
use clvmr::serde::node_to_bytes_backrefs;
use clvmr::{ChiaDialect, ClvmFlags};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::time::Instant;

/// Generate artificial high-demand CLVM programs as stress tests for the
/// interpreter
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Generate calls to the specified operator opcode
    #[arg(short, long)]
    operator: u32,

    /// Pass these arguments to the operator. This command line option take
    /// multiple arguments. Arguments are specified in the number
    /// of bytes of arbitrary bytes to be passed. If an argument is prefixed by
    /// "n", it will be a nested call to the same operator. i.e. the result of
    /// the operator will be passed in as the argument. The argument size will
    /// be used for the base case, once "depth" nested calls have been made.
    #[arg(long, num_args=0..)]
    args: Vec<String>,

    /// Depth of nesting calls
    #[arg(short, long, default_value = "100")]
    depth: u32,

    /// Form a cons-list of calls, with the specified length
    #[arg(short, long)]
    cons: Option<u32>,

    /// write resulting program (in hex-form) to the specified file
    #[arg(long)]
    output: Option<String>,

    /// When set, set the cost limit to 11 billion, without subtracting the
    /// byte cost
    #[arg(long)]
    ignore_byte_cost: bool,

    /// Run the resulting program
    #[arg(long)]
    run: bool,
}

enum Parameter {
    Value(NodePtr),
    NestingCall(NodePtr),
}

fn build_call(a: &mut Allocator, opcode: u32, args: &Vec<Parameter>, depth: u32) -> NodePtr {
    let mut parameters = a.nil();
    for arg in args.iter().rev() {
        match arg {
            Parameter::Value(v) => parameters = a.new_pair(*v, parameters).expect("new_pair"),
            Parameter::NestingCall(v) if depth == 0 => {
                parameters = a.new_pair(*v, parameters).expect("new_pair");
            }
            Parameter::NestingCall(_) => {
                let v = build_call(a, opcode, args, depth - 1);
                parameters = a.new_pair(v, parameters).expect("new_pair");
            }
        }
    }

    let op_code = a.new_number(opcode.into()).expect("new_number");
    a.new_pair(op_code, parameters).expect("new_pair")
}

fn get_value<R: Rng>(
    a: &mut Allocator,
    constants: &mut HashMap<u32, NodePtr>,
    rng: &mut R,
    atom_size: &str,
) -> NodePtr {
    let value_size = atom_size
        .parse::<u32>()
        .expect("failed to parse argument size, expect integer");
    match constants.entry(value_size) {
        Entry::Occupied(e) => *e.get(),
        Entry::Vacant(e) => {
            let mut bytes = vec![0_u8; value_size as usize];
            rng.fill_bytes(bytes.as_mut_slice());
            // make values positive
            bytes[0] &= 0x7f;
            let atom = a.new_atom(&bytes).expect("new_atom");
            let quoted_atom = a.new_pair(a.one(), atom).expect("new_pair");
            e.insert(quoted_atom);
            quoted_atom
        }
    }
}

pub fn main() {
    let options = Args::parse();

    let mut rng = StdRng::seed_from_u64(0x1337);
    let mut constants = HashMap::<u32, NodePtr>::new();
    let mut a = Allocator::new();

    let args = options
        .args
        .iter()
        .map(|v| -> Parameter {
            if &v[0..1] == "n" {
                Parameter::NestingCall(get_value(&mut a, &mut constants, &mut rng, &v[1..]))
            } else {
                Parameter::Value(get_value(&mut a, &mut constants, &mut rng, v))
            }
        })
        .collect();

    let call = build_call(&mut a, options.operator, &args, options.depth);

    // If we're supposed to make a cons list of the call, do that now
    // cons list can be combined with nested calls
    let program = if let Some(cons) = options.cons {
        let cons_node = a.new_number(4.into()).expect("new_number");
        let mut cons_list = call;
        for _ in 0..cons {
            let args = a.new_pair(cons_list, NodePtr::NIL).expect("new_pair");
            let args = a.new_pair(call, args).expect("new_pair");
            cons_list = a.new_pair(cons_node, args).expect("new_pair");
        }
        cons_list
    } else {
        call
    };

    let bytes = node_to_bytes_backrefs(&a, program).expect("serialize");
    if let Some(output) = options.output {
        let hex = hex::encode(&bytes);
        std::fs::write(output, hex).expect("write to output file");
    }

    if options.run {
        #[cfg(debug_assertions)]
        {
            println!("WARNING: Running debug build");
        }

        let mut max_cost = 11_000_000_000_u64;
        if bytes.len() as u64 * 12_000 > max_cost {
            println!(
                "byte cost exceeds generator limit: {}",
                bytes.len() * 12_000
            );
        }

        if !options.ignore_byte_cost {
            max_cost = max_cost.saturating_sub(bytes.len() as u64 * 12_000);
        }

        let dialect = ChiaDialect::new(ClvmFlags::empty());
        let start = Instant::now();
        let Reduction(cost, _) =
            clvmr::run_program(&mut a, &dialect, program, NodePtr::NIL, 20_000_000_000)
                .expect("program failed");
        let duration = start.elapsed();
        println!(
            "program took {:0.3} s to run, cost: {cost} ({:.2}%) (max: {max_cost})",
            duration.as_millis() as f64 / 1000.0,
            cost as f64 / max_cost as f64 * 100.0
        );
        if cost > max_cost {
            println!("cost exceeded max: {max_cost}");
        }
    }
}
