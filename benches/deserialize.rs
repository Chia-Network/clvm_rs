use clvmr::allocator::Allocator;
use clvmr::serde::{node_from_bytes_backrefs, node_from_bytes_backrefs_old};
use clvmr::serde_2026::{deserialize_2026, serialize_2026};

const BENCH_MAX_ATOM_LEN: usize = 1 << 20;
use criterion::{Criterion, criterion_group, criterion_main};
use std::include_bytes;
use std::time::Instant;

fn deserialize_benchmark(c: &mut Criterion) {
    let blocks = [
        ("0", include_bytes!("0.generator") as &[u8]),
        ("1", include_bytes!("1.generator") as &[u8]),
        ("2", include_bytes!("2.generator") as &[u8]),
        ("3", include_bytes!("3.generator") as &[u8]),
        ("4", include_bytes!("4.generator") as &[u8]),
    ];

    let mut group = c.benchmark_group("deserialize");

    for (name, block) in blocks {
        // Legacy format benches (backrefs)
        {
            let mut a = Allocator::new();
            let iter_checkpoint = a.checkpoint();

            group.bench_function(format!("node_from_bytes_backrefs {name}"), |b| {
                b.iter(|| {
                    a.restore_checkpoint(&iter_checkpoint);
                    let start = Instant::now();
                    node_from_bytes_backrefs(&mut a, block).expect("node_from_bytes_backrefs");
                    start.elapsed()
                })
            });

            group.bench_function(format!("node_from_bytes_backrefs_old {name}"), |b| {
                b.iter(|| {
                    a.restore_checkpoint(&iter_checkpoint);
                    let start = Instant::now();
                    node_from_bytes_backrefs_old(&mut a, block)
                        .expect("node_from_bytes_backrefs_old");
                    start.elapsed()
                })
            });
        }

        // serde_2026 format: convert from legacy, then benchmark deserialization
        {
            let mut a = Allocator::new();
            let node = node_from_bytes_backrefs(&mut a, block).expect("node_from_bytes_backrefs");
            let serialized_2026 = serialize_2026(&a, node, 0).expect("serialize_2026");

            let mut a = Allocator::new();
            let iter_checkpoint = a.checkpoint();
            group.bench_function(format!("deserialize_2026 {name}"), |b| {
                b.iter(|| {
                    a.restore_checkpoint(&iter_checkpoint);
                    let start = Instant::now();
                    deserialize_2026(&mut a, &serialized_2026, BENCH_MAX_ATOM_LEN, false)
                        .expect("deserialize_2026");
                    start.elapsed()
                })
            });
        }
    }

    group.finish();
}

criterion_group!(deserialize, deserialize_benchmark);
criterion_main!(deserialize);
