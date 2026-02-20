use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::{ChiaDialect, ClvmFlags};
use clvmr::reduction::Reduction;
use clvmr::run_program::run_program;
use clvmr::serde::{node_from_bytes, node_to_bytes};
use std::fs::File;
use std::io::Write;
use std::time::Instant;

/*
This file is for comparing the native sha256tree with the clvm implementation
The costs for the native implementation should be lower as it is not required to
make allocations.

This file also outputs the timings for both the native and clvm versions so we
can check that the costs are closely aligned with the actual work done on the
CPU.
*/

// this function calculates the cost per node theoretically
// for a perfectly balanced binary tree
fn time_complete_tree(a: &mut Allocator, sha_prog: NodePtr, leaf_size: usize, output_file: &str) {
    let dialect = ChiaDialect::new(ClvmFlags::ENABLE_SHA256_TREE.union(ClvmFlags::ENABLE_GC));
    let op_code = a.new_small_number(63).unwrap();
    let quote = a.one();

    // leaf atom (not pre-processed)
    let mut tree = a.new_atom(&vec![0xff; leaf_size]).unwrap();

    let mut output = File::create(output_file).expect("failed to open file");

    let mut leaf_count = 1;
    for i in 1..13 {
        leaf_count *= 2;
        // double the number of leaves each iteration
        tree = a.new_pair(tree, tree).unwrap();

        let checkpoint = a.checkpoint();

        let q = a.new_pair(quote, tree).unwrap();
        let call = a.new_pair(q, a.nil()).unwrap();
        let call = a.new_pair(op_code, call).unwrap();

        // native
        let start = Instant::now();
        let Reduction(cost_native, result_native) =
            run_program(a, &dialect, call, a.nil(), 11_000_000_000).unwrap();
        let duration_native = start.elapsed().as_nanos();
        let result_native = node_to_bytes(a, result_native).expect("node_to_bytes");

        // clvm
        let start = Instant::now();
        let Reduction(cost_clvm, result_clvm) =
            run_program(a, &dialect, sha_prog, tree, 11_000_000_000).unwrap();
        let duration_clvm = start.elapsed().as_nanos();

        let result_clvm = node_to_bytes(a, result_clvm).expect("node_to_bytes");
        assert_eq!(result_clvm, result_native);

        writeln!(
            output,
            "{leaf_count}\t{duration_native}\t{cost_native}\t{duration_clvm}\t{cost_clvm}"
        )
        .expect("write to file");

        a.restore_checkpoint(&checkpoint);

        if i == 9 {
            println!("\ncost for hashing complete tree (leaf size: {leaf_size})");
            println!("           time     cost");
            println!("Native: {:-7}  {:-7}", duration_native, cost_native);
            println!("CLVM:   {:-7}  {:-7}", duration_clvm, cost_clvm);
            println!(
                "ratio:    {:.1}%    {:.1}%",
                duration_native as f64 / duration_clvm as f64 * 100.0,
                cost_native as f64 / cost_clvm as f64 * 100.0
            );
        }
    }
}

fn write_plot(log_file: &str, variant: &str, output: &mut impl Write) {
    writeln!(
        output,
        "
set term svg
set output \"sha256tree_bench-{variant}.svg\"
set title \"CLVM implementation vs. native comparison ({variant})\"
set xlabel \"Number of leaves\"
set ylabel \"Time (ns)\"
set y2label \"Cost\"
plot \"{log_file}\" using 1:2 with points title \"native time\" axis x1y1, \\
   \"{log_file}\" using 1:3 with lines title \"native cost\" axis x1y2, \\
   \"{log_file}\" using 1:4 with points title \"CLVM time\" axis x1y1, \\
   \"{log_file}\" using 1:5 with lines title \"CLVM cost\" axis x1y2
"
    )
    .expect("failed to write to file");
}

fn main() {
    let shaprogbytes = hex::decode(
        "ff02ffff01ff02ff02ffff04ff02ffff04ff03ff80808080ffff04ffff01ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff02ffff04ff02ffff04ff09ff80808080ffff02ff02ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080"
    ).unwrap();

    let mut a = Allocator::new();
    let shaprog = node_from_bytes(&mut a, shaprogbytes.as_ref()).unwrap();

    let mut gnuplot = File::create("plots.gnuplots").expect("failed to open file");

    for leaf_size in &[0, 2, 1000, 100000] {
        let log_file = format!("measurements/shatree-compare-{leaf_size}.dat");
        time_complete_tree(&mut a, shaprog, *leaf_size, &log_file);
        write_plot(&log_file, &format!("{leaf_size}-byte-atoms"), &mut gnuplot);
    }

    println!("\nData + plots complete. Generate graphs with:");
    println!("    gnuplot plots.gnuplot\n");
}
