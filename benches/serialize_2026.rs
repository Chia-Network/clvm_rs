use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde_2026::serialize_2026;
use criterion::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use std::include_bytes;
use std::time::Instant;

fn serialize_2026_benchmark(c: &mut Criterion) {
    let generators = [
        ("0", include_bytes!("0.generator") as &[u8]),
        ("1", include_bytes!("1.generator") as &[u8]),
        ("2", include_bytes!("2.generator") as &[u8]),
        ("3", include_bytes!("3.generator") as &[u8]),
        ("4", include_bytes!("4.generator") as &[u8]),
    ];

    let mut group = c.benchmark_group("serialize_2026");

    for (name, block) in generators {
        let mut a = Allocator::new();
        let node = node_from_bytes_backrefs(&mut a, block).expect("node_from_bytes_backrefs");

        group.bench_function(format!("serialize_2026 {name}"), |b| {
            b.iter(|| {
                let start = Instant::now();
                black_box(serialize_2026(&a, node, 0).expect("serialize_2026"));
                start.elapsed()
            })
        });
    }

    group.finish();
}

criterion_group!(benches, serialize_2026_benchmark);
criterion_main!(benches);
