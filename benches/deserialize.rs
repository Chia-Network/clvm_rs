use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::serialized_length_from_bytes;
use clvmr::serde::serialized_length_from_bytes_trusted;
use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use std::include_bytes;
use std::time::Instant;

fn deserialize_benchmark(c: &mut Criterion) {
    let block = include_bytes!("block_af9c3d98.bin");

    let mut group = c.benchmark_group("deserialize");
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("serialized_length_from_bytes", |b| {
        b.iter(|| {
            let start = Instant::now();
            let _ = serialized_length_from_bytes(block);
            start.elapsed()
        })
    });

    group.bench_function("serialized_length_from_bytes_trusted", |b| {
        b.iter(|| {
            let start = Instant::now();
            let _ = serialized_length_from_bytes_trusted(block);
            start.elapsed()
        })
    });

    let mut a = Allocator::new();
    let iter_checkpoint = a.checkpoint();

    group.bench_function("node_from_bytes_backrefs", |b| {
        b.iter(|| {
            a.restore_checkpoint(&iter_checkpoint);
            let start = Instant::now();
            let _ = node_from_bytes_backrefs(&mut a, block);
            start.elapsed()
        })
    });

    group.bench_function("node_from_bytes", |b| {
        b.iter(|| {
            a.restore_checkpoint(&iter_checkpoint);
            let start = Instant::now();
            let _ = node_from_bytes(&mut a, block);
            start.elapsed()
        })
    });

    group.finish();
}

criterion_group!(deserialize, deserialize_benchmark);
criterion_main!(deserialize);
