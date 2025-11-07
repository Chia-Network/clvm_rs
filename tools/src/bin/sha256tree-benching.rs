use std::fs::File;
use std::io::Write;
use std::time::Instant;

use linreg::linear_regression_of;

// bring in your existing code
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::treehash::{tree_hash, tree_hash_atom};

fn make_nested_pairs(a: &mut Allocator, depth: usize) -> NodePtr {
    let mut node = a.nil();
    for _ in 0..depth {
        node = a.new_pair(node, a.nil()).unwrap();
    }
    node
}

fn make_list_of_atoms(a: &mut Allocator, n: usize) -> NodePtr {
    let atom = a.new_atom(&[1u8; 32]).unwrap();
    let mut list = a.nil();
    for _ in 0..n {
        list = a.new_pair(atom, list).unwrap();
    }
    list
}

fn main() -> std::io::Result<()> {
    let mut output = File::create("sha256tree_costs.tsv")?;

    writeln!(output, "# type\tx\ty")?;

    // cost call (with nil)
    let mut samples = vec![];
    {
        let mut a = Allocator::new();
        for depth in 1..1000 {
            let node = make_nested_pairs(&mut a, depth);
            let start = Instant::now();
            tree_hash(&a, node); // using tree hash as it costs the same as cached
            let t = start.elapsed().as_secs_f64();
            writeln!(output, "call\t{}\t{}", depth, t)?;
            samples.push((depth as f64, t));
        }
        let (slope, intercept): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
        writeln!(output, "\n# call_slope\t{:.9}", slope)?;
        writeln!(output, "# call_intercept\t{:.9}\n", intercept)?;
        println!("call slope: {:.9}, intercept: {:.9}", slope, intercept);
    }

    // cost atom sizes
    let mut samples = vec![];
    {
        for size in 1..8192 {
            let atom: Vec<u8> = vec![11_u8; size];
            let start = Instant::now();
            tree_hash_atom(&atom);
            let t = start.elapsed().as_secs_f64();
            writeln!(output, "atom\t{}\t{}", size, t)?;
            samples.push((size as f64, t));
        }
        let (slope, intercept): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
        writeln!(output, "\n# atom_slope\t{:.9}", slope)?;
        writeln!(output, "# atom_intercept\t{:.9}\n", intercept)?;
        println!("atom slope: {:.9}, intercept: {:.9}", slope, intercept);
    }

    // cost list of atoms
    let mut samples = vec![];
    {
        let mut a = Allocator::new();
        for n in 1..256 {
            let node = make_list_of_atoms(&mut a, n);
            let start = Instant::now();
            tree_hash(&a, node);
            let t = start.elapsed().as_secs_f64();
            writeln!(output, "pair\t{}\t{}", n, t)?;
            samples.push((n as f64, t));
        }
        let (slope, intercept): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
        writeln!(output, "\n# pair_slope\t{:.9}", slope)?;
        writeln!(output, "# pair_intercept\t{:.9}\n", intercept)?;
        println!("pair slope: {:.9}, intercept: {:.9}", slope, intercept);
    }

    println!("Results written to sha256tree_costs.tsv");
    Ok(())
}
