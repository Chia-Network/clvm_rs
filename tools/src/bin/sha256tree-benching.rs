use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::{ChiaDialect, ENABLE_SHA256_TREE};
use clvmr::run_program::run_program;
use clvmr::serde::node_from_bytes;
use linreg::linear_regression_of;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

fn time_per_byte_for_atom(
    a: &mut Allocator,
    sha_prog: NodePtr,
    mut output_native_time: impl Write,
    mut output_clvm_time: impl Write,
    mut output_native_cost: impl Write,
    mut output_clvm_cost: impl Write,
) -> (
    (f64, f64),
    (f64, f64), // time slopes
    (f64, f64),
    (f64, f64), // cost slopes
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
        atom.extend(std::iter::repeat(((i % 89) + 10) as u8).take(32));

        let atom_node = a.new_atom(&atom).unwrap();
        let args = a.new_pair(quote, atom_node).unwrap();
        let call = a.new_pair(args, a.nil()).unwrap();
        let call = a.new_pair(op_code, call).unwrap();

        let checkpoint = a.checkpoint();

        // native
        let start = Instant::now();
        let cost = run_program(a, &dialect, call, a.nil(), 11_000_000_000)
            .unwrap()
            .0;
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_native_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_native_cost, "{}\t{}", i, cost).unwrap();
        samples_time_native.push((i as f64, duration));
        samples_cost_native.push((i as f64, cost as f64));

        // clvm
        a.restore_checkpoint(&checkpoint);
        let start = Instant::now();
        let cost = run_program(a, &dialect, sha_prog, atom_node, 11_000_000_000)
            .unwrap()
            .0;
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_clvm_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_clvm_cost, "{}\t{}", i, cost).unwrap();
        samples_time_clvm.push((i as f64, duration));
        samples_cost_clvm.push((i as f64, cost as f64));
    }

    (
        linear_regression_of(&samples_time_native).unwrap(),
        linear_regression_of(&samples_time_clvm).unwrap(),
        linear_regression_of(&samples_cost_native).unwrap(),
        linear_regression_of(&samples_cost_clvm).unwrap(),
    )
}

fn time_per_cons_for_list(
    a: &mut Allocator,
    sha_prog: NodePtr,
    mut output_native_time: impl Write,
    mut output_clvm_time: impl Write,
    mut output_native_cost: impl Write,
    mut output_clvm_cost: impl Write,
) -> (
    (f64, f64),
    (f64, f64), // time slopes
    (f64, f64),
    (f64, f64), // cost slopes
) {
    let mut samples_time_native = Vec::<(f64, f64)>::new();
    let mut samples_time_clvm = Vec::<(f64, f64)>::new();
    let mut samples_cost_native = Vec::<(f64, f64)>::new();
    let mut samples_cost_clvm = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_SHA256_TREE);

    let op_code = a.new_small_number(63).unwrap();
    let quote = a.one();
    let mut list = a.nil();

    for _ in 0..500 {
        list = a.new_pair(a.nil(), list).unwrap();
    }

    for i in 0..1000 {
        list = a.new_pair(a.nil(), list).unwrap();
        let q = a.new_pair(quote, list).unwrap();
        let call = a.new_pair(q, a.nil()).unwrap();
        let call = a.new_pair(op_code, call).unwrap();

        let checkpoint = a.checkpoint();

        // native
        let start = Instant::now();
        let cost = run_program(a, &dialect, call, a.nil(), 11_000_000_000)
            .unwrap()
            .0;
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_native_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_native_cost, "{}\t{}", i, cost).unwrap();
        samples_time_native.push((i as f64, duration));
        samples_cost_native.push((i as f64, cost as f64));

        // clvm
        a.restore_checkpoint(&checkpoint);
        let start = Instant::now();
        let cost = run_program(a, &dialect, sha_prog, list, 11_000_000_000)
            .unwrap()
            .0;
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_clvm_time, "{}\t{}", i, duration).unwrap();
        writeln!(output_clvm_cost, "{}\t{}", i, cost).unwrap();
        samples_time_clvm.push((i as f64, duration));
        samples_cost_clvm.push((i as f64, cost as f64));
    }

    (
        linear_regression_of(&samples_time_native).unwrap(),
        linear_regression_of(&samples_time_clvm).unwrap(),
        linear_regression_of(&samples_cost_native).unwrap(),
        linear_regression_of(&samples_cost_clvm).unwrap(),
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
        cons_native_time,
        cons_clvm_time,
        cons_native_cost,
        cons_clvm_cost,
    );

    println!("atom results: ");
    println!("Native time slope  (ns): {:.4}", atom_nat_t.0);
    println!("CLVM   time slope  (ns): {:.4}", atom_clvm_t.0);
    println!("Native cost slope      : {:.4}", atom_nat_c.0);
    println!("CLVM   cost slope      : {:.4}", atom_clvm_c.0);

    println!("list results: ");
    println!("Native time slope  (ns): {:.4}", cons_nat_t.0);
    println!("CLVM   time slope  (ns): {:.4}", cons_clvm_t.0);
    println!("Native cost slope      : {:.4}", cons_nat_c.0);
    println!("CLVM   cost slope      : {:.4}", cons_clvm_c.0);

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
