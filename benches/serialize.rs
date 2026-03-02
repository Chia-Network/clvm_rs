use clvmr::allocator::{Allocator, NodePtr};
use clvmr::serde::{
    Serializer, node_from_bytes, node_from_bytes_backrefs, node_to_bytes_backrefs,
    node_to_bytes_limit,
};
use criterion::black_box;
use criterion::{Criterion, criterion_group, criterion_main};
use std::include_bytes;
use std::time::Duration;
use std::time::Instant;

fn serialize_benchmark(c: &mut Criterion) {
    // the blocks are serialized with back-refs. In order to accurately measure
    // the cost of the compression itself, we first need to inflate them and
    // then serialize again.
    let block0: &[u8] = include_bytes!("0.generator");
    let block1: &[u8] = include_bytes!("1.generator");
    let block2: &[u8] = include_bytes!("2.generator");
    let block3: &[u8] = include_bytes!("3.generator");
    let block4: &[u8] = include_bytes!("4.generator");

    let mut group = c.benchmark_group("serialize");

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
                let mut ser = Serializer::new(None);
                let _ = ser.add(&a, node);
                black_box(ser.into_inner());
                start.elapsed()
            })
        });
    }

    group.finish();
}

fn many_spends_benchmark(c: &mut Criterion) {
    let mut a = Allocator::new();

    let num_spends: u64 = 20000;
    let sentinel = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();

    let mut spend_list = sentinel;

    for i in 0..num_spends {
        let mut parent_id = [0u8; 32];
        parent_id[24..32].copy_from_slice(&i.to_be_bytes());

        // solution
        let item = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();
        // amount
        let amount = a.new_number(1_000_000.into()).unwrap();
        let item = a.new_pair(amount, item).unwrap();
        // puzzle reveal
        let puzzle = a.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();
        let item = a.new_pair(puzzle, item).unwrap();
        // parent-id
        let parent_id = a.new_atom(&parent_id).unwrap();
        let item = a.new_pair(parent_id, item).unwrap();
        spend_list = a.new_pair(item, spend_list).unwrap();
    }

    let mut group = c.benchmark_group("many_spends");

    group.warm_up_time(Duration::from_nanos(1));
    group.sample_size(10);

    // The following function is too slow to run on CI.
    // Benchmarking many_spends/node_to_bytes_backrefs: Warming up for 1.0000 ns
    // Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 775.1s.
    // many_spends/node_to_bytes_backrefs
    //                         time:   [64.284 s 67.147 s 70.449 s]
    // Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 10.0s.
    // many_spends/incremental_serializer
    //                         time:   [942.38 ms 952.31 ms 962.57 ms]
    //                         change: [-2.2538% -0.9775% +0.3944%] (p = 0.17 > 0.05)
    //                         No change in performance detected.
    /*
        group.bench_function("node_to_bytes_backrefs", |b| {
            b.iter(|| {
                black_box(node_to_bytes_backrefs(&a, spend_list).expect("node_to_bytes_backrefs"));
            })
        });
    */
    group.bench_function("incremental_serializer", |b| {
        b.iter(|| {
            let mut ser = Serializer::new(Some(sentinel));
            let (done, _) = ser.add(&a, spend_list).unwrap();
            assert!(!done);
            let (done, _) = ser.add(&a, NodePtr::NIL).unwrap();
            assert!(done);
            black_box(ser.into_inner());
        })
    });
}

criterion_group!(serialize, serialize_benchmark, many_spends_benchmark);
criterion_main!(serialize);
