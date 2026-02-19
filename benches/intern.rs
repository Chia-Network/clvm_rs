use clvmr::allocator::Allocator;
use clvmr::serde::{intern, node_from_bytes, node_from_bytes_backrefs, node_to_bytes_limit};
use criterion::{Criterion, criterion_group, criterion_main};
use std::include_bytes;
use std::time::Instant;

fn intern_benchmark(c: &mut Criterion) {
    let block0: &[u8] = include_bytes!("0.generator");
    let block1: &[u8] = include_bytes!("1.generator");
    let block2: &[u8] = include_bytes!("2.generator");
    let block3: &[u8] = include_bytes!("3.generator");
    let block4: &[u8] = include_bytes!("4.generator");

    let mut group = c.benchmark_group("intern");

    for (block, name) in [
        (&block0, "0"),
        (&block1, "1"),
        (&block2, "2"),
        (&block3, "3"),
        (&block4, "4"),
    ] {
        let mut a = Allocator::new();
        let node = node_from_bytes_backrefs(&mut a, block).expect("node_from_bytes_backrefs");

        // if the inflated form takes too much space, just run the benchmark on the compact form
        let node = if let Ok(inflated) = node_to_bytes_limit(&a, node, 2000000) {
            a = Allocator::new();
            node_from_bytes(&mut a, inflated.as_slice()).expect("node_from_bytes")
        } else {
            node
        };

        group.bench_function(format!("intern {name}"), |b| {
            b.iter(|| {
                let start = Instant::now();
                let _tree = intern(&a, node).expect("intern");
                start.elapsed()
            })
        });
    }

    group.finish();
}

criterion_group!(intern_bench, intern_benchmark);
criterion_main!(intern_bench);
