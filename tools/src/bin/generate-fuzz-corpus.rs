use clvmr::serde::write_atom::write_atom;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use sha1::{Digest, Sha1};
use std::fs::{create_dir_all, File};
use std::io::Write;

#[repr(u8)]
#[derive(PartialEq, Clone, Copy, Debug)]
enum Type {
    Program,
    Tree,
    List,
    Bool,
    Int64,
    Int32,
    Zero,
    Cost,
    Bytes32,
    Bytes48,
    Bytes96,
    Bytes288,
    AnyAtom,
}

const ATOMS: [Type; 9] = [
    Type::Bool,
    Type::Int64,
    Type::Int32,
    Type::Zero,
    Type::Cost,
    Type::Bytes32,
    Type::Bytes48,
    Type::Bytes96,
    Type::Bytes288,
];

struct OperatorInfo {
    opcode: u8,
    result: Type,
    operands: &'static [Type],
}

const fn op(opcode: u8, operands: &'static [Type], result: Type) -> OperatorInfo {
    OperatorInfo {
        opcode,
        result,
        operands,
    }
}

const OPERATORS: [OperatorInfo; 33] = [
    // apply
    op(2, &[Type::Program, Type::Tree], Type::AnyAtom),
    // if
    op(
        3,
        &[Type::Bool, Type::Program, Type::Program],
        Type::Program,
    ),
    // cons
    op(4, &[Type::AnyAtom, Type::List], Type::List),
    // first
    op(5, &[Type::List], Type::AnyAtom),
    // rest
    op(6, &[Type::List], Type::List),
    // listp
    op(7, &[Type::List], Type::Bool),
    // raise
    op(8, &[Type::AnyAtom], Type::AnyAtom),
    // equal
    op(9, &[Type::AnyAtom, Type::AnyAtom], Type::Bool),
    // greater-bytes
    op(10, &[Type::AnyAtom, Type::AnyAtom], Type::Bool),
    // sha256
    op(
        11,
        &[Type::AnyAtom, Type::AnyAtom, Type::AnyAtom],
        Type::Bytes32,
    ),
    // substr
    op(12, &[Type::AnyAtom, Type::Int32], Type::AnyAtom),
    op(
        12,
        &[Type::AnyAtom, Type::Int32, Type::Int32],
        Type::AnyAtom,
    ),
    // strlen
    op(13, &[Type::AnyAtom], Type::Int32),
    // concat
    op(14, &[Type::AnyAtom, Type::AnyAtom], Type::AnyAtom),
    // add
    op(16, &[Type::Int64, Type::Int64], Type::Int64),
    // subtract
    op(17, &[Type::Int64, Type::Int64], Type::Int64),
    // multiply
    op(18, &[Type::Int64, Type::Int64], Type::Int64),
    // div
    op(19, &[Type::Int64, Type::Int64], Type::Int64),
    // divmod
    op(20, &[Type::Int64, Type::Int64], Type::List),
    // gr
    op(21, &[Type::Int64, Type::Int64], Type::Bool),
    // ash
    op(22, &[Type::Int64, Type::Int32], Type::Int64),
    // lsh
    op(23, &[Type::Int64, Type::Int32], Type::Int64),
    // logand
    op(24, &[Type::AnyAtom, Type::AnyAtom], Type::AnyAtom),
    // logior
    op(25, &[Type::AnyAtom, Type::AnyAtom], Type::AnyAtom),
    // logxor
    op(26, &[Type::AnyAtom, Type::AnyAtom], Type::AnyAtom),
    // lognot
    op(27, &[Type::AnyAtom], Type::AnyAtom),
    // point_add
    op(29, &[Type::Bytes48, Type::Bytes48], Type::Bytes48),
    // pubkey for exp
    op(30, &[Type::AnyAtom], Type::Bytes48),
    // not
    op(32, &[Type::AnyAtom], Type::Bool),
    // AnyAtom
    op(33, &[Type::AnyAtom, Type::AnyAtom], Type::Bool),
    // all
    op(34, &[Type::AnyAtom, Type::AnyAtom], Type::Bool),
    // softfork
    op(
        36,
        &[Type::Cost, Type::Zero, Type::Program, Type::Tree],
        Type::Bool,
    ),
    // BLS extensions

    // coinid
    op(
        48,
        &[Type::Bytes32, Type::Bytes32, Type::Int64],
        Type::Bytes32,
    ),
];

const ZEROS: [u8; 288] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

fn rand_atom_type<R: Rng>(rng: &mut R) -> Type {
    ATOMS[rng.gen_range(0..ATOMS.len())]
}

fn sample<'a, R: Rng, T>(rng: &mut R, vec: &'a [T]) -> &'a T {
    &vec[rng.gen_range(0..vec.len())]
}

const INTERESTING_U32: [u32; 9] = [
    0, 1, 5, 0xff, 0xffff, 0x100, 0xffffffff, 0x7fffffff, 0x800000,
];

const INTERESTING_U64: [u64; 8] = [
    0,
    1,
    5,
    0xff,
    0xffffffffffffffff,
    0x100,
    0x8000000000000000,
    0x7fffffffffffffff,
];

fn generate_u32<R: Rng>(rng: &mut R) -> u32 {
    *sample(rng, &INTERESTING_U32)
}

fn generate_u64<R: Rng>(rng: &mut R) -> u64 {
    *sample(rng, &INTERESTING_U64)
}

fn type_convertible(from: Type, to: Type) -> bool {
    from == to
        || to == Type::AnyAtom && ATOMS.contains(&from)
        || to == Type::Tree && from == Type::List
        || to == Type::Zero && from == Type::Int32
        || to == Type::Cost && from == Type::Int64
}

fn generate_program<R: Rng>(op: &OperatorInfo, rng: &mut R, buffer: &mut Vec<u8>) {
    buffer.push(0xff); // cons
    buffer.push(op.opcode);
    for arg in op.operands {
        buffer.push(0xff); // cons

        if rng.gen_bool(0.3) {
            // an expression yielding the type "arg"
            // pick all operators
            let potential_ops: Vec<&OperatorInfo> = OPERATORS
                .iter()
                .filter(|o| type_convertible(o.result, *arg))
                .collect();
            if potential_ops.is_empty() {
                println!("no operator returns {:?}", arg);
            }
            let sub_op = sample(rng, &potential_ops);
            generate_program(sub_op, rng, buffer);
        } else {
            // quoted value
            buffer.push(0xff); // cons
            buffer.push(1); // quote
            generate(*arg, rng, buffer);
        }
    }
    buffer.push(0x80); // cons
}

fn generate_args<R: Rng>(op: &OperatorInfo, rng: &mut R, buffer: &mut Vec<u8>) {
    for arg in op.operands {
        buffer.push(0xff); // cons
                           // quoted value
        buffer.push(0xff); // cons
        buffer.push(1); // quote
        generate(*arg, rng, buffer);
    }
    buffer.push(0x80); // cons
}

fn generate<R: Rng>(t: Type, rng: &mut R, buffer: &mut Vec<u8>) {
    match t {
        Type::Tree => {
            buffer.push(0xff); // cons
                               // 10% to keep growing the tree
            let left_side = if rng.gen_bool(0.1) {
                Type::Tree
            } else {
                rand_atom_type(rng)
            };
            let right_side = if rng.gen_bool(0.1) {
                Type::Tree
            } else {
                rand_atom_type(rng)
            };
            generate(left_side, rng, buffer);
            generate(right_side, rng, buffer);
        }
        Type::List => {
            let len = rng.gen_range(0..10);
            for _i in 0..len {
                buffer.push(0xff); // cons
                generate(rand_atom_type(rng), rng, buffer);
            }
            buffer.push(0x80); // NIL
        }
        Type::Program => {
            let op = sample(rng, &OPERATORS);
            generate_program(op, rng, buffer);
        }
        Type::Bool => {
            if rng.gen_bool(0.5) {
                buffer.push(0x80);
            } else {
                buffer.push(1);
            }
        }
        Type::Int64 => {
            write_atom(buffer, &generate_u64(rng).to_be_bytes()).expect("write_atom failed");
        }
        Type::Int32 => {
            write_atom(buffer, &generate_u32(rng).to_be_bytes()).expect("write_atom failed");
        }
        Type::Zero => {
            buffer.push(0x80);
        }
        Type::Cost => {
            write_atom(buffer, &8000000000_u64.to_be_bytes()).expect("write_atom failed");
        }
        Type::Bytes32 => {
            write_atom(buffer, &ZEROS[..32]).expect("write_atom failed");
        }
        Type::Bytes48 => {
            write_atom(buffer, &ZEROS[..48]).expect("write_atom failed");
        }
        Type::Bytes96 => {
            write_atom(buffer, &ZEROS[..96]).expect("write_atom failed");
        }
        Type::Bytes288 => {
            write_atom(buffer, &ZEROS[..288]).expect("write_atom failed");
        }
        Type::AnyAtom => {
            generate(rand_atom_type(rng), rng, buffer);
        }
    }
}

fn filename(buffer: &[u8]) -> String {
    let mut sha1 = Sha1::new();
    sha1.update(buffer);
    hex::encode(sha1.finalize())
}

pub fn main() {
    let mut buffer = Vec::<u8>::new();
    let mut rng = StdRng::seed_from_u64(0x1337);

    create_dir_all("../fuzz/corpus/fuzz_run_program").expect("failed to create directory");
    create_dir_all("../fuzz/corpus/operators").expect("failed to create directory");

    for i in 0..20000 {
        buffer.truncate(0);

        let op = &OPERATORS[i % OPERATORS.len()];
        generate_program(op, &mut rng, &mut buffer);
        let mut out = File::create(format!(
            "../fuzz/corpus/fuzz_run_program/{}",
            filename(&buffer)
        ))
        .expect("failed to open file");
        out.write_all(&buffer).expect("failed to write file");
    }

    for i in 0..20000 {
        buffer.truncate(0);

        let op = &OPERATORS[i % OPERATORS.len()];
        generate_args(op, &mut rng, &mut buffer);
        let mut out = File::create(format!("../fuzz/corpus/operators/{}", filename(&buffer)))
            .expect("failed to open file");
        out.write_all(&buffer).expect("failed to write file");
    }
}
