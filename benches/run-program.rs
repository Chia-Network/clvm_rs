use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::ChiaDialect;
use clvmr::serde::node_from_bytes;
use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use std::fs::read_to_string;
use std::time::Instant;

fn long_strings(a: &mut Allocator) -> NodePtr {
    let mut list = a.null();
    for _i in 0..1000 {
        let item = a
            .new_atom(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")
            .expect("new_atom");
        list = a.new_pair(item, list).expect("new_pair");
    }

    a.new_pair(list, a.null()).expect("new_pair")
}

fn large_tree_impl(a: &mut Allocator, depth: i32) -> NodePtr {
    if depth == 0 {
        a.new_atom(b"foobar").expect("new_atom")
    } else {
        let left = large_tree_impl(a, depth - 1);
        let right = large_tree_impl(a, depth - 1);
        a.new_pair(left, right).expect("new_pair")
    }
}

fn large_tree<const DEPTH: i32>(a: &mut Allocator) -> NodePtr {
    large_tree_impl(a, DEPTH)
}

fn long_string(a: &mut Allocator) -> NodePtr {
    let mut atom = Vec::with_capacity(62000);
    for _i in 0..1000 {
        atom.extend(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    }
    let item = a.new_atom(&atom[..]).expect("new_atom");
    a.new_pair(item, a.null()).expect("new_pair")
}

fn tuple3<const N: i32, const K: i32>(a: &mut Allocator) -> NodePtr {
    let list = a.null();
    let item = a.new_number(K.into()).expect("new_atom");
    let list = a.new_pair(item, list).expect("new_pair");
    let item = a.new_number(N.into()).expect("new_atom");
    let list = a.new_pair(item, list).expect("new_pair");
    let item = a.new_atom(b"BCDEFGH").expect("new_atom");
    a.new_pair(item, list).expect("new_pair")
}

fn pair<const N: i32>(a: &mut Allocator) -> NodePtr {
    let list = a.null();
    let item = a.new_number(N.into()).expect("new_atom");
    let list = a.new_pair(item, list).expect("new_pair");
    let item = a
        .new_atom(&[0xb, 0xad, 0xf0, 0x0d, 0xfe, 0xed, 0xfa, 0xce])
        .expect("new_atom");
    a.new_pair(item, list).expect("new_pair")
}

fn single_value<const N: i32>(a: &mut Allocator) -> NodePtr {
    let list = a.null();
    let item = a.new_number(N.into()).expect("new_atom");
    a.new_pair(item, list).expect("new_pair")
}

fn generate_list<const N: i32>(a: &mut Allocator) -> NodePtr {
    let mut list = a.null();
    for _i in 0..N {
        let item = a.new_number(42.into()).expect("new_atom");
        list = a.new_pair(item, list).expect("new_pair");
    }
    a.new_pair(list, a.null()).expect("new_pair")
}

fn large_block(a: &mut Allocator) -> NodePtr {
    let mut buffer = Vec::<u8>::with_capacity(1000);
    for i in 0..1032 {
        buffer.push((i & 0xff) as u8);
    }

    let mut list = a.null();
    for i in 0..1000 {
        let hex_key1 = hex::encode(&buffer[i..i + 32]);
        let hex_key2 = hex::encode(&buffer[i / 2..i / 2 + 32]);
        let hex_key3 = hex::encode(&buffer[i / 3..i / 3 + 32]);
        let hex_key4 = hex::encode(&buffer[i..i + 3]);
        let fmt = format!(
            "ffa0\
{hex_key1}\
ffff\
ff02ffff01ff02ffff01ff02ffff03ff0bffff01ff02ffff03ffff09ff05ffff\
1dff0bffff1effff0bff0bffff02ff06ffff04ff02ffff04ff17ff8080808080\
808080ffff01ff02ff17ff2f80ffff01ff088080ff0180ffff01ff04ffff04ff\
04ffff04ff05ffff04ffff02ff06ffff04ff02ffff04ff17ff80808080ff8080\
8080ffff02ff17ff2f808080ff0180ffff04ffff01ff32ff02ffff03ffff07ff\
0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080ff\
ff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff0580\
80ff0180ff018080ffff04ffff01a0\
{hex_key2}\
ff018080\
ffff80ffff01ffff33ffa0\
{hex_key3}\
ff83\
{hex_key4}\
8080ff80808080\
"
        );
        let puzzle = hex::decode(fmt).expect("failed to parse puzzle");
        let puzzle = node_from_bytes(a, &puzzle[..]).expect("failed to parse puzzle");
        list = a.new_pair(puzzle, list).expect("new_pair");
    }

    // quote
    a.new_pair(a.one(), list).expect("new_pair")
}

fn matrix<const W: i32, const H: i32>(a: &mut Allocator) -> NodePtr {
    let mut args = a.null();

    for _l in 0..2 {
        let mut col = a.null();

        for _k in 0..H {
            let mut row = a.null();
            for _i in 0..W {
                let val = a.new_atom(b"ccba9401").expect("new_atom");
                row = a.new_pair(val, row).expect("new_pair");
            }

            col = a.new_pair(row, col).expect("new_pair");
        }

        args = a.new_pair(col, args).expect("new_pair");
    }
    args
}

fn prev_generator(a: &mut Allocator) -> NodePtr {
    node_from_bytes(
        a,
        &hex::decode(
            "ffff02ffff01ff05ffff02ff3effff04ff02ffff04ff05ff8080808080ffff\
04ffff01ffffff81ff7fff81df81bfffffff02ffff03ffff09ff0bffff018180\
80ffff01ff04ff80ffff04ff05ff808080ffff01ff02ffff03ffff0aff0bff18\
80ffff01ff02ff1affff04ff02ffff04ffff02ffff03ffff0aff0bff1c80ffff\
01ff02ffff03ffff0aff0bff1480ffff01ff08ffff018c62616420656e636f64\
696e6780ffff01ff04ffff0effff18ffff011fff0b80ffff0cff05ff80ffff01\
018080ffff04ffff0cff05ffff010180ff80808080ff0180ffff01ff04ffff18\
ffff013fff0b80ffff04ff05ff80808080ff0180ff80808080ffff01ff04ff0b\
ffff04ff05ff80808080ff018080ff0180ff04ffff0cff15ff80ff0980ffff04\
ffff0cff15ff0980ff808080ffff04ffff04ff05ff1380ffff04ff2bff808080\
ffff02ff16ffff04ff02ffff04ff09ffff04ffff02ff3effff04ff02ffff04ff\
15ff80808080ff8080808080ff02ffff03ffff09ffff0cff05ff80ffff010180\
ff1080ffff01ff02ff2effff04ff02ffff04ffff02ff3effff04ff02ffff04ff\
ff0cff05ffff010180ff80808080ff80808080ffff01ff02ff12ffff04ff02ff\
ff04ffff0cff05ffff010180ffff04ffff0cff05ff80ffff010180ff80808080\
8080ff0180ff018080ffc189ff01ffffffa00000000000000000000000000000\
000000000000000000000000000000000000ff830186a080ffffff02ffff01ff\
02ffff01ff02ffff03ff0bffff01ff02ffff03ffff09ff05ffff1dff0bffff1e\
ffff0bff0bffff02ff06ffff04ff02ffff04ff17ff8080808080808080ffff01\
ff02ff17ff2f80ffff01ff088080ff0180ffff01ff04ffff04ff04ffff04ff05\
ffff04ffff02ff06ffff04ff02ffff04ff17ff80808080ff80808080ffff02ff\
17ff2f808080ff0180ffff04ffff01ff32ff02ffff03ffff07ff0580ffff01ff\
0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080ffff02ff06ffff\
04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff01\
8080ffff04ffff01b081963921826355dcb6c355ccf9c2637c18adf7d38ee44d\
803ea9ca41587e48c913d8d46896eb830aeadfc13144a8eac3ff018080ffff80\
ffff01ffff33ffa06b7a83babea1eec790c947db4464ab657dbe9b887fe9acc2\
47062847b8c2a8a9ff830186a08080ff808080808080",
        )
        .expect("invalid generator hex")[..],
    )
    .expect("failed to parse generator")
}

fn none(a: &mut Allocator) -> NodePtr {
    a.null()
}

fn point_pow(a: &mut Allocator) -> NodePtr {
    let list = a.null();
    let item = a.new_number(1337.into()).expect("new_atom");
    let list = a.new_pair(item, list).expect("new_pair");
    let item = a.new_atom(&hex::decode("b3b8ac537f4fd6bde9b26221d49b54b17a506be147347dae5d081c0a6572b611d8484e338f3432971a9823976c6a232b").expect("invalid point hex")).expect("new_atom");
    a.new_pair(item, list).expect("new_pair")
}

type EnvFn = fn(&mut Allocator) -> NodePtr;

fn run_program_benchmark(c: &mut Criterion) {
    let mut a = Allocator::new();
    let dialect = ChiaDialect::new(0);

    let test_case_checkpoint = a.checkpoint();

    let mut group = c.benchmark_group("run_program");
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);

    for (test, make_env) in &[
        ("block-2000", none as EnvFn),
        ("compressed-2000", prev_generator),
        ("concat", tuple3::<16, 397>),
        ("count-even", generate_list::<15000>),
        ("factorial", single_value::<10000>),
        ("hash-string", long_strings),
        ("hash-tree", large_tree::<16>),
        ("large-block", large_block),
        ("loop_add", single_value::<4000000>),
        ("loop_ior", single_value::<4000000>),
        ("loop_not", single_value::<4000000>),
        ("loop_sub", single_value::<4000000>),
        ("loop_xor", single_value::<4000000>),
        ("matrix-multiply", matrix::<50, 50>),
        ("point-pow", point_pow),
        ("pubkey-tree", large_tree::<10>),
        ("shift-left", pair::<410>),
        ("substr", long_string),
        ("substr-tree", long_string),
        ("sum-tree", large_tree::<19>),
    ] {
        a.restore_checkpoint(&test_case_checkpoint);

        println!("benchmark/{test}.hex");
        let prg = read_to_string(format!("benchmark/{test}.hex"))
            .expect("failed to load benchmark program");
        let prg = hex::decode(prg.trim()).expect("invalid hex in benchmark program");
        let prg = node_from_bytes(&mut a, &prg[..]).expect("failed to parse benchmark program");
        let env = make_env(&mut a);
        let iter_checkpoint = a.checkpoint();
        group.bench_function(*test, |b| {
            b.iter(|| {
                a.restore_checkpoint(&iter_checkpoint);
                let start = Instant::now();
                clvmr::run_program(&mut a, &dialect, prg, env, 11000000000)
                    .expect("benchmark program failed");
                start.elapsed()
            })
        });
    }

    group.finish();
}

criterion_group!(run_program, run_program_benchmark);
criterion_main!(run_program);
