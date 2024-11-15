use criterion::{criterion_group, criterion_main, Criterion};

use chia_sha2::Sha256;

const BYTE_LENGTHS: [u8; 6] = [8, 16, 32, 64, 96, 128];
const MAX_VAL: u8 = 250;

fn gen_bytes(value: u8, amount: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..amount {
        bytes.push(value);
    }
    bytes
}

fn hash_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut sha256 = Sha256::new();
    sha256.update(bytes);
    sha256.finalize()
}

fn sha256_hash_benchmark(c: &mut Criterion) {
    // setup benchmark
    let mut group = c.benchmark_group("sha256_hash");

    group.bench_function("hash_benchmark", |b| {
        b.iter(|| {
            // this figures out how many iterations to run.
            for val in 0..MAX_VAL {
                for len in BYTE_LENGTHS {
                    let bytes = gen_bytes(val, len);
                    hash_bytes(&bytes);
                }
            }
        })
    });
    // create
    group.finish();
}

criterion_group!(sha256_hash, sha256_hash_benchmark);
criterion_main!(sha256_hash);
