use clvmr::allocator::Allocator;
use clvmr::serde::{
    node_from_bytes, node_from_bytes_backrefs, node_from_bytes_backrefs_old,
    node_to_bytes_backrefs, serialized_length_from_bytes, serialized_length_from_bytes_trusted,
    tree_hash_from_stream,
};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::include_bytes;
use std::time::Duration;

fn deserialize_benchmark(c: &mut Criterion) {
    let block = include_bytes!("block_af9c3d98.bin");
    let compressed_block = {
        let mut a = Allocator::new();
        let input = node_from_bytes(&mut a, block).expect("failed to parse input file");
        node_to_bytes_backrefs(&a, input).expect("failed to compress generator")
    };

    let mut group = c.benchmark_group("deserialize");

    for (bl, name_suffix) in &[
        (block as &[u8], ""),
        (compressed_block.as_ref(), "-compressed"),
    ] {
        group.bench_function(format!("serialized_length_from_bytes{name_suffix}"), |b| {
            b.iter(|| black_box(serialized_length_from_bytes(bl).expect("serialized_length_from_bytes")))
        });

        group.bench_function(
            format!("serialized_length_from_bytes_trusted{name_suffix}"),
            |b| {
                b.iter(|| {
                    black_box(
                        serialized_length_from_bytes_trusted(bl)
                            .expect("serialized_length_from_bytes_trusted"),
                    )
                })
            },
        );

        // we don't support compressed CLVM in tree_hash_from_stream yet
        if name_suffix.is_empty() {
            group.bench_function(format!("tree_hash_from_stream{name_suffix}"), |b| {
                b.iter(|| {
                    let mut cur = std::io::Cursor::new(*bl);
                    black_box(tree_hash_from_stream(&mut cur).expect("tree_hash_from_stream"))
                })
            });
        }

        let mut a = Allocator::new();
        let iter_checkpoint = a.checkpoint();

        group.bench_function(format!("node_from_bytes_backrefs{name_suffix}"), |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    a.restore_checkpoint(&iter_checkpoint);
                    let start = std::time::Instant::now();
                    black_box(
                        node_from_bytes_backrefs(&mut a, bl).expect("node_from_bytes_backrefs"),
                    );
                    total += start.elapsed();
                }
                total
            })
        });

        group.bench_function(format!("node_from_bytes_backrefs_old{name_suffix}"), |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    a.restore_checkpoint(&iter_checkpoint);
                    let start = std::time::Instant::now();
                    black_box(
                        node_from_bytes_backrefs_old(&mut a, bl)
                            .expect("node_from_bytes_backrefs_old"),
                    );
                    total += start.elapsed();
                }
                total
            })
        });
    }

    let mut a = Allocator::new();
    let iter_checkpoint = a.checkpoint();
    group.bench_function("node_from_bytes", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                a.restore_checkpoint(&iter_checkpoint);
                let start = std::time::Instant::now();
                black_box(node_from_bytes(&mut a, block).expect("node_from_bytes"));
                total += start.elapsed();
            }
            total
        })
    });

    group.finish();
}

criterion_group!(deserialize, deserialize_benchmark);
criterion_main!(deserialize);
