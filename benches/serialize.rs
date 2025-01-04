use clvmr::allocator::Allocator;
use clvmr::serde::{
    node_from_bytes, node_from_bytes_backrefs, node_to_bytes, node_to_bytes_backrefs, Serializer,
};
use criterion::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use std::include_bytes;
use std::time::Instant;

fn serialize_benchmark(c: &mut Criterion) {
    // the blocks are serialized with back-refs. In order to accurately measure
    // the cost of the compression itself, we first need to inflate them and
    // then serialize again.
    let block0: &[u8] = include_bytes!("0.generator");
    let block1: &[u8] = include_bytes!("1.generator");
    let block2: &[u8] = include_bytes!("2.generator");

    let mut group = c.benchmark_group("serialize");

    for (block, name) in [(&block0, "0"), (&block1, "1"), (&block2, "2")] {
        let mut a = Allocator::new();
        let node = node_from_bytes_backrefs(&mut a, block).expect("node_from_bytes_backrefs");
        let inflated = node_to_bytes(&a, node).expect("node_to_bytes");
        let mut a = Allocator::new();
        let node = node_from_bytes(&mut a, inflated.as_slice()).expect("node_from_bytes");

        group.bench_function(format!("node_to_bytes_backrefs {name}"), |b| {
            b.iter(|| {
                let start = Instant::now();
                black_box(node_to_bytes_backrefs(&a, node).expect("node_to_bytes_backrefs"));
                start.elapsed()
            })
        });

        group.bench_function(format!("Serializer {name}"), |b| {
            b.iter(|| {
                let start = Instant::now();
                let mut ser = Serializer::default();
                let _ = ser.add(&a, node, None);
                black_box(ser.into_inner());
                start.elapsed()
            })
        });
    }

    group.finish();
}

criterion_group!(serialize, serialize_benchmark);
criterion_main!(serialize);
