use clvmr::chia_dialect::ChiaDialect;
use clvmr::run_program::run_program;
use std::fs::File;
use std::io::Write;
use std::time::Instant;

use linreg::linear_regression_of;

// bring in your existing code
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::treehash::tree_hash;

fn make_list_of_atoms(a: &mut Allocator, n: usize) -> NodePtr {
    let atom = a.new_atom(&[1u8; 32]).unwrap();
    let mut list = a.nil();
    for _ in 0..n {
        list = a.new_pair(atom, list).unwrap();
    }
    list
}

fn time_per_byte_for_atom(a: &mut Allocator, output: &mut dyn Write) -> (f64, f64) {
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(0x0200); // enable shatree

    let op_code = a.new_number(65.into()).unwrap();
    let quote = a.new_number(1.into()).unwrap();
    let mut atom_str = String::from("");
    let checkpoint = a.checkpoint();

    for i in (0..1000000).step_by(5) {
        // make the atom longer as a function of i
        atom_str.push_str(&((i % 89) + 10).to_string()); // just to mix it up
        let atom = a.new_atom(&hex::decode(&atom_str).unwrap()).unwrap();
        // let args = a.new_pair(atom, a.nil()).unwrap();
        let args = a.new_pair(quote, atom).unwrap();
        let call = a.new_pair(args, a.nil()).unwrap();
        let call = a.new_pair(op_code, call).unwrap();
        let start = Instant::now();
        run_program(a, &dialect, call, a.nil(), 11000000000).unwrap();
        let duration = start.elapsed();
        let sample = (i as f64, duration.as_nanos() as f64);
        writeln!(output, "{}\t{}", sample.0, sample.1).expect("failed to write");
        samples.push(sample);

        a.restore_checkpoint(&checkpoint);
    }

    linear_regression_of(&samples).expect("linreg failed")
}

fn main() -> std::io::Result<()> {
    let mut output = File::create("sha256tree_costs.tsv")?;

    writeln!(output, "# type\tx\ty")?;
    // this "magic" scaling depends on the computer you run the tests on.
    // It's calibrated against the timing of point_add, which has a cost
    let cost_scale = ((101094.0 / 39000.0) + (1343980.0 / 131000.0)) / 2.0;

    // base call cost is covered in benchmark-clvm-cost so not included here
    // cost atom sizes
    {
        let mut a = Allocator::new();
        let (slope, intercept) = time_per_byte_for_atom(&mut a, &mut output);
        let cost = slope * cost_scale;
        writeln!(output, "\n# atom_slope\t{:.9}", slope)?;
        writeln!(output, "\n# atom_slope * cost_scale\t{:?}", cost)?;
        writeln!(output, "# atom_intercept\t{:.9}\n", intercept)?;
        println!(
            "atom slope: {:.9}, intercept: {:.9}, cost (slope * cost_scale): {:.9}",
            slope, intercept, cost
        );
    }

    // cost list of atoms
    let mut samples = vec![];
    {
        let mut a = Allocator::new();
        for n in 1..256 {
            let node = make_list_of_atoms(&mut a, n);
            let start = Instant::now();
            tree_hash(&a, node);
            let t = start.elapsed().as_nanos() as f64;
            writeln!(output, "pair\t{}\t{}", n, t)?;
            samples.push((n as f64, t));
        }
        let (slope, intercept): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
        let cost = slope * cost_scale;
        writeln!(output, "\n# pair_slope\t{:.9}", slope)?;
        writeln!(output, "\n# pair_slope * cost_scale\t{:?}", cost)?;
        writeln!(output, "# pair_intercept\t{:.9}\n", intercept)?;
        println!(
            "pair slope: {:.9}, intercept: {:.9}, cost: {:.9}",
            slope, intercept, cost
        );
    }

    println!("Results written to sha256tree_costs.tsv");
    Ok(())
}
