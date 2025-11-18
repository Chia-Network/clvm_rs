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
    mut output_native: impl Write,
    mut output_clvm: impl Write,
) -> ((f64, f64), (f64, f64)) {
    let mut samples = Vec::<(f64, f64)>::new();
    let mut samples_clvm = Vec::<(f64, f64)>::new();
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
        let start = Instant::now();
        run_program(a, &dialect, call, a.nil(), 11000000000).unwrap();
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_native, "{}\t{}", i, duration).unwrap();
        samples.push((i as f64, duration));

        a.restore_checkpoint(&checkpoint);
        let start = Instant::now();
        run_program(a, &dialect, sha_prog, atom_node, 11000000000).unwrap();
        let duration = start.elapsed().as_nanos() as f64;
        writeln!(output_clvm, "{}\t{}", i, duration).unwrap();
        samples_clvm.push((i as f64, duration));
    }

    (
        linear_regression_of(&samples).unwrap(),
        linear_regression_of(&samples_clvm).unwrap(),
    )
}

fn time_per_cons_for_list(
    a: &mut Allocator,
    sha_prog: NodePtr,
    mut output_native: impl Write,
    mut output_clvm: impl Write,
) -> ((f64, f64), (f64, f64)) {
    let mut samples_native = Vec::<(f64, f64)>::new();
    let mut samples_clvm = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_SHA256_TREE);

    let op_code = a.new_small_number(63).unwrap();
    let quote = a.one();
    let mut list = a.nil();

    for _ in 0..500 {
        list = a.new_pair(a.nil(), list).unwrap();
    }

    for i in 0..10_00 {
        list = a.new_pair(a.nil(), list).unwrap();

        let quoted = a.new_pair(quote, list).unwrap();
        let call = a.new_pair(quoted, a.nil()).unwrap();
        let call = a.new_pair(op_code, call).unwrap();

        // native
        let checkpoint = a.checkpoint();
        let start = Instant::now();
        run_program(a, &dialect, call, a.nil(), 11000000000).unwrap();
        let t = start.elapsed().as_nanos() as f64;
        writeln!(output_native, "{}\t{}", i, t).unwrap();
        samples_native.push((i as f64, t));

        // clvm
        a.restore_checkpoint(&checkpoint);
        let start = Instant::now();
        run_program(a, &dialect, sha_prog, list, 11000000000).unwrap();
        let t = start.elapsed().as_nanos() as f64;
        writeln!(output_clvm, "{}\t{}", i, t).unwrap();
        samples_clvm.push((i as f64, t));
    }

    (
        linear_regression_of(&samples_native).unwrap(),
        linear_regression_of(&samples_clvm).unwrap(),
    )
}

fn main() {
    let shaprogbytes = hex::decode(
        "ff02ffff01ff02ff02ffff04ff02ffff04ff03ff80808080ffff04ffff01ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff02ffff04ff02ffff04ff09ff80808080ffff02ff02ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080"
    ).unwrap();

    let mut a = Allocator::new();
    let shaprog = node_from_bytes(&mut a, shaprogbytes.as_ref()).unwrap();

    let atom_native = BufWriter::new(File::create("atom_native.dat").unwrap());
    let atom_clvm = BufWriter::new(File::create("atom_clvm.dat").unwrap());
    let cons_native = BufWriter::new(File::create("cons_native.dat").unwrap());
    let cons_clvm = BufWriter::new(File::create("cons_clvm.dat").unwrap());

    let (atom_nat_lin, atom_clvm_lin) =
        time_per_byte_for_atom(&mut a, shaprog, atom_native, atom_clvm);

    let (cons_nat_lin, cons_clvm_lin) =
        time_per_cons_for_list(&mut a, shaprog, cons_native, cons_clvm);

    println!("Atom native slope: {:.4}", atom_nat_lin.0);
    println!("Atom CLVM slope: {:.4}", atom_clvm_lin.0);
    println!("List native slope: {:.4}", cons_nat_lin.0);
    println!("List CLVM slope: {:.4}", cons_clvm_lin.0);

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

set output "cons_bench.png"
set title "Time per Cons Cell (List SHA-tree)"
set xlabel "Iteration"
set ylabel "Time (ns)"
plot \
    "cons_native.dat" using 1:2 with lines title "native", \
    "cons_clvm.dat" using 1:2 with lines title "clvm"
"#
    )
    .unwrap();

    println!("Run with: gnuplot plots.gnuplot");
}
