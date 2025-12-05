use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::{ChiaDialect, ENABLE_SHA256_TREE};
use clvmr::run_program::run_program;
use clvmr::serde::{node_from_bytes, node_to_bytes};
use linreg::linear_regression_of;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

/*
This file is for comparing the native sha256tree with the clvm implementation which previously existed.
The costs for the native implementation should be lower as it is not required to make allocations.

This file also outputs the timings for both the native and clvm versions so we can check that the costs
are closely aligned with the actual work done on the CPU.
*/

// this function is for comparing the cost per 32byte chunk of hashing between the native and clvm implementation
#[allow(clippy::type_complexity)]
fn time_per_byte_for_atom(
    a: &mut Allocator,
    sha_prog: NodePtr,
    mut output_native_time: impl Write,
    mut output_clvm_time: impl Write,
    mut output_native_cost: impl Write,
    mut output_clvm_cost: impl Write,
) -> (
    f64,
    f64, // time slopes
    f64,
    f64, // cost slopes
) {
    let mut samples_time_native = Vec::<(f64, f64)>::new();
    let mut samples_time_clvm = Vec::<(f64, f64)>::new();
    let mut samples_cost_native = Vec::<(f64, f64)>::new();
    let mut samples_cost_clvm = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_SHA256_TREE);

    let op_code = a.new_small_number(63).unwrap();
    let quote = a.one();
    let mut atom = vec![0xff; 10_000];

    for i in 0..10_000 {
        atom.extend(std::iter::repeat_n(((i % 89) + 10) as u8, 32));

        let atom_node = a.new_atom(&atom).unwrap();
        let args = a.new_pair(quote, atom_node).unwrap();
        let call = a.new_pair(args, a.nil()).unwrap();
        let call = a.new_pair(op_code, call).unwrap();

        let checkpoint = a.checkpoint();

        // native
        let start = Instant::now();
        let red = run_program(a, &dialect, call, a.nil(), 11_000_000_000).unwrap();
        let cost = red.0;
        let result_1 = node_to_bytes(a, red.1).expect("should work");
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_native_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_native_cost, "{}\t{}", i, cost).unwrap();
        samples_time_native.push((i as f64, duration));
        samples_cost_native.push((i as f64, cost as f64));

        // clvm
        a.restore_checkpoint(&checkpoint);
        let start = Instant::now();
        let red = run_program(a, &dialect, sha_prog, atom_node, 11_000_000_000).unwrap();
        let cost = red.0;
        let result_2 = node_to_bytes(a, red.1).expect("should work");
        assert_eq!(result_1, result_2);
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_clvm_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_clvm_cost, "{}\t{}", i, cost).unwrap();
        samples_time_clvm.push((i as f64, duration));
        samples_cost_clvm.push((i as f64, cost as f64));
    }

    (
        linear_regression_of(&samples_time_native).unwrap().0,
        linear_regression_of(&samples_time_clvm).unwrap().0,
        linear_regression_of(&samples_cost_native).unwrap().0,
        linear_regression_of(&samples_cost_clvm).unwrap().0,
    )
}

// this function calculates the cost per node theoretically
// in reality we are only charging per hash operation on a 32 byte chunk
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
fn time_per_cons_for_list(
    a: &mut Allocator,
    sha_prog: NodePtr,
    bytes32_native_cost: f64,
    bytes32_clvm_cost: f64,
    bytes32_native_time: f64,
    bytes32_clvm_time: f64,
    mut output_native_time: impl Write,
    mut output_clvm_time: impl Write,
    mut output_native_cost: impl Write,
    mut output_clvm_cost: impl Write,
) -> (
    f64,
    f64, // time slopes
    f64,
    f64, // cost slopes
) {
    let mut samples_time_native = Vec::<(f64, f64)>::new();
    let mut samples_time_clvm = Vec::<(f64, f64)>::new();
    let mut samples_cost_native = Vec::<(f64, f64)>::new();
    let mut samples_cost_clvm = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_SHA256_TREE);

    let op_code = a.new_small_number(63).unwrap();
    let quote = a.one();
    let mut list = a.nil();

    let atom = a.new_atom(&[0xff, 0xff]).unwrap();

    for _ in 0..500 {
        list = a.new_pair(atom, list).unwrap();
    }

    for i in 0..1000 {
        list = a.new_pair(atom, list).unwrap();
        let q = a.new_pair(quote, list).unwrap();
        let call = a.new_pair(q, a.nil()).unwrap();
        let call = a.new_pair(op_code, call).unwrap();

        let checkpoint = a.checkpoint();

        // native
        let start = Instant::now();
        let red = run_program(a, &dialect, call, a.nil(), 11_000_000_000).unwrap();
        let cost = red.0;
        let result_1 = node_to_bytes(a, red.1).expect("should work");
        let duration = start.elapsed().as_nanos() as f64;
        // a new list entry is 2 nodes (a cons and a nil) and a 3 chunk hash operation and a 1 chunk hash operation
        // this equation lets us figure out a theoretical cost just for a node
        let duration = (duration - (4.0 * bytes32_native_time)) / 2.0;
        let cost = (cost as f64 - (4.0 * bytes32_native_cost)) / 2.0;
        writeln!(output_native_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_native_cost, "{}\t{}", i, cost).unwrap();
        samples_time_native.push((i as f64, duration));
        samples_cost_native.push((i as f64, cost as f64));

        // clvm
        a.restore_checkpoint(&checkpoint);
        let start = Instant::now();
        let red = run_program(a, &dialect, sha_prog, list, 11_000_000_000).unwrap();
        let cost = red.0;
        let result_2 = node_to_bytes(a, red.1).expect("should work");
        assert_eq!(result_1, result_2);
        let duration = start.elapsed().as_nanos() as f64;
        // a new list entry is 2 nodes (a cons and a nil) and a 3 chunk hash operation and a 1 chunk hash operation
        // this equation lets us figure out a theoretical cost just for a node
        let duration = (duration - (4.0 * bytes32_clvm_time)) / 2.0;
        let cost = (cost as f64 - (4.0 * bytes32_clvm_cost)) / 2.0;
        writeln!(output_clvm_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_clvm_cost, "{}\t{}", i, cost).unwrap();
        samples_time_clvm.push((i as f64, duration));
        samples_cost_clvm.push((i as f64, cost as f64));
    }

    (
        linear_regression_of(&samples_time_native).unwrap().0,
        linear_regression_of(&samples_time_clvm).unwrap().0,
        linear_regression_of(&samples_cost_native).unwrap().0,
        linear_regression_of(&samples_cost_clvm).unwrap().0,
    )
}

fn main() {
    let shaprogbytes = hex::decode(
        "ff02ffff01ff02ff02ffff04ff02ffff04ff03ff80808080ffff04ffff01ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff02ffff04ff02ffff04ff09ff80808080ffff02ff02ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080"
    ).unwrap();

    let mut a = Allocator::new();
    let shaprog = node_from_bytes(&mut a, shaprogbytes.as_ref()).unwrap();

    // Output files
    let atom_native_time = BufWriter::new(File::create("atom_native.dat").unwrap());
    let atom_clvm_time = BufWriter::new(File::create("atom_clvm.dat").unwrap());
    let cons_native_time = BufWriter::new(File::create("cons_native.dat").unwrap());
    let cons_clvm_time = BufWriter::new(File::create("cons_clvm.dat").unwrap());

    let atom_native_cost = BufWriter::new(File::create("atom_native_cost.dat").unwrap());
    let atom_clvm_cost = BufWriter::new(File::create("atom_clvm_cost.dat").unwrap());
    let cons_native_cost = BufWriter::new(File::create("cons_native_cost.dat").unwrap());
    let cons_clvm_cost = BufWriter::new(File::create("cons_clvm_cost.dat").unwrap());

    let (atom_nat_t, atom_clvm_t, atom_nat_c, atom_clvm_c) = time_per_byte_for_atom(
        &mut a,
        shaprog,
        atom_native_time,
        atom_clvm_time,
        atom_native_cost,
        atom_clvm_cost,
    );

    let (cons_nat_t, cons_clvm_t, cons_nat_c, cons_clvm_c) = time_per_cons_for_list(
        &mut a,
        shaprog,
        atom_nat_c,
        atom_clvm_c,
        atom_nat_t,
        atom_clvm_t,
        cons_native_time,
        cons_clvm_time,
        cons_native_cost,
        cons_clvm_cost,
    );

    // taken from benchmark-clvm-cost.rs
    let cost_scale = ((101094.0 / 39000.0) + (1343980.0 / 131000.0)) / 2.0;

    println!("Costs per bytes32 chunk: ");
    println!("Native time per bytes32  (ns): {:.4}", atom_nat_t);
    println!("CLVM   time per bytes32  (ns): {:.4}", atom_clvm_t);
    println!(
        "Native (time_per_bytes32  * cost_ratio): {:.4}",
        atom_nat_t * cost_scale
    );
    println!(
        "CLVM   (time_per_bytes  * cost_ratio : {:.4}",
        atom_clvm_t * cost_scale
    );

    println!("Native cost per bytes32      : {:.4}", atom_nat_c);
    println!("CLVM   cost per bytes32      : {:.4}", atom_clvm_c);

    // this is described as estimated as we're adding a cons and a nil atom each time
    // and then we're subtracting the costs to calculate what a single node might theoretically cost
    println!("Estimated costs per node results: ");
    println!("Native time per node  (ns): {:.4}", cons_nat_t);
    println!("CLVM   time per node  (ns): {:.4}", cons_clvm_t);
    println!(
        "Native (time_per_node  * cost_ratio): {:.4}",
        cons_nat_t * cost_scale
    );
    println!(
        "CLVM   (time_per_node * cost_ratio): {:.4}",
        cons_clvm_t * cost_scale
    );
    println!("Native cost per node      : {:.4}", cons_nat_c);
    println!("CLVM   cost per node      : {:.4}", cons_clvm_c);

    // gnuplot script
    let mut gp = File::create("plots.gnuplot").unwrap();
    writeln!(
        gp,
        r#"
set terminal png size 1200,900

set output "atom_bench.png"
set title "Time per Byte (Atom SHA-tree)"
set xlabel "Iteration"
set ylabel "Time (ns)"
plot \
    "atom_native.dat" using 1:2 with lines title "native", \
    "atom_clvm.dat" using 1:2 with lines title "clvm"

set output "atom_cost.png"
set title "Cost per Byte (Atom SHA-tree)"
set xlabel "Iteration"
set ylabel "Cost"
plot \
    "atom_native_cost.dat" using 1:2 with lines title "native", \
    "atom_clvm_cost.dat" using 1:2 with lines title "clvm"

set output "cons_bench.png"
set title "Time per Cons Cell (List SHA-tree)"
set xlabel "Iteration"
set ylabel "Time (ns)"
plot \
    "cons_native.dat" using 1:2 with lines title "native", \
    "cons_clvm.dat" using 1:2 with lines title "clvm"

set output "cons_cost.png"
set title "Cost per Cons Cell (List SHA-tree)"
set xlabel "Iteration"
set ylabel "Cost"
plot \
    "cons_native_cost.dat" using 1:2 with lines title "native", \
    "cons_clvm_cost.dat" using 1:2 with lines title "clvm"
"#
    )
    .unwrap();

    println!("\nData + plots complete. Generate graphs with:");
    println!("    gnuplot plots.gnuplot\n");
}
