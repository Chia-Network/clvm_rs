use clvmr::serde::write_atom::write_atom;
use hex_literal::hex;
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
    G1Point,
    G2Point,
    Sec1,
    Sig,
    Bytes20,
    Bytes32,
    Bytes96,
    AnyAtom,
}

const ATOMS: [Type; 12] = [
    Type::Bool,
    Type::Int64,
    Type::Int32,
    Type::Zero,
    Type::Cost,
    Type::G1Point,
    Type::G2Point,
    Type::Sec1,
    Type::Sig,
    Type::Bytes20,
    Type::Bytes32,
    Type::Bytes96,
];

struct OperatorInfo {
    opcode: u32,
    result: Type,
    operands: &'static [Type],
}

const fn op(opcode: u32, operands: &'static [Type], result: Type) -> OperatorInfo {
    OperatorInfo {
        opcode,
        result,
        operands,
    }
}

const OPERATORS: [OperatorInfo; 83] = [
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
    op(14, &[Type::Int64, Type::Int64, Type::Int32], Type::Bytes20),
    op(
        14,
        &[Type::Bytes32, Type::Bytes32, Type::Bytes32],
        Type::Bytes96,
    ),
    op(
        14,
        &[Type::AnyAtom, Type::AnyAtom, Type::AnyAtom],
        Type::AnyAtom,
    ),
    // add
    op(16, &[], Type::Int64),
    op(16, &[Type::Int64], Type::Int64),
    op(16, &[Type::Int64, Type::Int64], Type::Int64),
    op(16, &[Type::Int64, Type::Int64, Type::Int64], Type::Int64),
    // subtract
    op(17, &[], Type::Int64),
    op(17, &[Type::Int64], Type::Int64),
    op(17, &[Type::Int64, Type::Int64], Type::Int64),
    op(17, &[Type::Int64, Type::Int64, Type::Int64], Type::Int64),
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
    op(24, &[], Type::AnyAtom),
    op(24, &[Type::AnyAtom], Type::AnyAtom),
    op(24, &[Type::AnyAtom, Type::AnyAtom], Type::AnyAtom),
    op(
        24,
        &[Type::AnyAtom, Type::AnyAtom, Type::AnyAtom],
        Type::AnyAtom,
    ),
    // logior
    op(25, &[], Type::AnyAtom),
    op(25, &[Type::AnyAtom], Type::AnyAtom),
    op(25, &[Type::AnyAtom, Type::AnyAtom], Type::AnyAtom),
    op(
        25,
        &[Type::AnyAtom, Type::AnyAtom, Type::AnyAtom],
        Type::AnyAtom,
    ),
    // logxor
    op(26, &[], Type::AnyAtom),
    op(26, &[Type::AnyAtom], Type::AnyAtom),
    op(26, &[Type::AnyAtom, Type::AnyAtom], Type::AnyAtom),
    op(
        26,
        &[Type::AnyAtom, Type::AnyAtom, Type::AnyAtom],
        Type::AnyAtom,
    ),
    // lognot
    op(27, &[Type::AnyAtom], Type::AnyAtom),
    // point_add
    op(29, &[], Type::G1Point),
    op(29, &[Type::G1Point], Type::G1Point),
    op(29, &[Type::G1Point, Type::G1Point], Type::G1Point),
    op(
        29,
        &[Type::G1Point, Type::G1Point, Type::G1Point],
        Type::G1Point,
    ),
    // pubkey for exp
    op(30, &[Type::AnyAtom], Type::G1Point),
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
    // bls_g1_subtract
    op(49, &[Type::G1Point, Type::G1Point], Type::G1Point),
    // bls_g1_multiply
    op(50, &[Type::G1Point, Type::Int64], Type::G1Point),
    op(50, &[Type::G1Point, Type::Bytes32], Type::G1Point),
    op(50, &[Type::G1Point, Type::Bytes96], Type::G1Point),
    // bls_g1_negate
    op(51, &[Type::G1Point], Type::G1Point),
    // bls_g2_add
    op(52, &[Type::G2Point, Type::G2Point], Type::G2Point),
    // bls_g2_subtract
    op(53, &[Type::G2Point, Type::G2Point], Type::G2Point),
    // bls_g2_multiply
    op(54, &[Type::G2Point, Type::Int64], Type::G2Point),
    op(54, &[Type::G2Point, Type::Bytes32], Type::G2Point),
    op(54, &[Type::G2Point, Type::Bytes96], Type::G2Point),
    // bls_g2_negate
    op(55, &[Type::G2Point], Type::G2Point),
    // bls_map_to_g1
    op(56, &[Type::AnyAtom, Type::AnyAtom], Type::G1Point),
    op(56, &[Type::AnyAtom], Type::G1Point),
    // bls_map_to_g2
    op(57, &[Type::AnyAtom, Type::AnyAtom], Type::G2Point),
    op(57, &[Type::AnyAtom], Type::G2Point),
    // bls_pairing_identity
    op(58, &[Type::G2Point, Type::G1Point], Type::Zero),
    op(
        58,
        &[Type::G2Point, Type::G1Point, Type::G2Point, Type::G1Point],
        Type::Zero,
    ),
    op(
        58,
        &[
            Type::G2Point,
            Type::G1Point,
            Type::G2Point,
            Type::G1Point,
            Type::G2Point,
            Type::G1Point,
        ],
        Type::Zero,
    ),
    // bls_verify
    op(59, &[Type::G2Point], Type::Zero),
    op(
        59,
        &[Type::G2Point, Type::G1Point, Type::Bytes20],
        Type::Zero,
    ),
    op(
        59,
        &[Type::G2Point, Type::G1Point, Type::AnyAtom],
        Type::Zero,
    ),
    op(
        59,
        &[
            Type::G2Point,
            Type::G1Point,
            Type::AnyAtom,
            Type::G1Point,
            Type::AnyAtom,
        ],
        Type::Zero,
    ),
    op(
        59,
        &[
            Type::G2Point,
            Type::G1Point,
            Type::AnyAtom,
            Type::G1Point,
            Type::AnyAtom,
            Type::G1Point,
            Type::AnyAtom,
        ],
        Type::Zero,
    ),
    // op_secp256k1_verify
    op(
        0x13d61f00,
        &[Type::Sec1, Type::Bytes32, Type::Sig],
        Type::Zero,
    ),
    // op_secp256r1_verify
    op(
        0x1c3a8f00,
        &[Type::Sec1, Type::Bytes32, Type::Sig],
        Type::Zero,
    ),
    // modpow
    op(60, &[Type::Int64, Type::Int64, Type::Int64], Type::Int64),
    op(
        60,
        &[Type::Bytes32, Type::Int64, Type::Bytes32],
        Type::Bytes32,
    ),
    // mod
    op(61, &[Type::Int64, Type::Int64], Type::Int64),
    op(61, &[Type::Bytes32, Type::Bytes32], Type::Bytes32),
];

const ZEROS: [u8; 96] = [0; 96];

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

const G1POINTS: [[u8; 48]; 3] = [
    hex!("c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    hex!("8b202593319bce41b090f3309986de59861ab1e2ff32aef871d83f9aac232c7253c01f1f649c6f69879c441286319de4"),
    hex!("b7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"),
];

const G2POINTS: [[u8; 96]; 3] = [
    hex!("c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
    hex!("80c37921e62092ef55f85f9eccb21bd80cfaafc0bce9cbdd6999b1a8cabadc8f23720f0261efafaf53cbcc74580b9432007b66d824668900a94934f184bc41bf9ccf9ec141c6f7da610aa7296cd0a181ae8fe176b607aa4c367f15ee0cb985d7"),
    hex!("942adad4dbeadcfd75aaa11940a5e5e16a8d8e91742029a3944610635ccc0572eceeb1c89d8a0e904c5d30b9497e700312dee7b833535effef24953dbf8f8aa770e2f1a8e01d3b6f6844e01a635ed95664babe9d62a2572651d0258461c8ba00"),
];

const BYTES20: [[u8; 20]; 1] = [hex!("39cb1950dba19a7bee9924b5bd2b29f190ffe4ef")];

const BYTES32: [[u8; 32]; 2] = [
    hex!("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"),
    hex!("74c2941eb2ebe5aa4f2287a4c5e506a6290c045004058de97a7edf0122548668"),
];

const SEC1: [&[u8]; 2] = [
    &hex!("02888b0c110ef0b4962e3fc6929cbba7a8bb25b4b2c885f55c76365018c909b439"),
    &hex!("0437a1674f3883b7171a11a20140eee014947b433723cf9f181a18fee4fcf96056103b3ff2318f00cca605e6f361d18ff0d2d6b817b1fa587e414f8bb1ab60d2b9"),
];

const SIG: [[u8;64]; 2] = [
    hex!("1acb7a6e062e78ccd4237b12c22f02b5a8d9b33cb3ba13c35e88e036baa1cbca75253bb9a96ffc48b43196c69c2972d8f965b1baa4e52348d8081cde65e6c018"),
    hex!("e8de121f4cceca12d97527cc957cca64a4bcfc685cffdee051b38ee81cb22d7e2c187fec82c731018ed2d56f08a4a5cbc40c5bfe9ae18c02295bb65e7f605ffc"),
];

fn type_convertible(from: Type, to: Type) -> bool {
    from == to
        || to == Type::AnyAtom && ATOMS.contains(&from)
        || to == Type::Tree && from == Type::List
        || to == Type::Zero && from == Type::Int32
        || to == Type::Cost && from == Type::Int64
}

fn write_int(buf: &mut Vec<u8>, val: u64) {
    if val == 0 {
        buf.push(0x80);
        return;
    }

    let bytes = val.to_be_bytes();

    // strip redundant leading zeroes
    let mut slice: &[u8] = &bytes;
    while slice.len() > 1 && slice[0] == 0 && (slice[1] & 0x80) == 0 {
        slice = &slice[1..];
    }
    write_atom(buf, slice).expect("write_atom failed");
}

fn generate_program<R: Rng>(op: &OperatorInfo, rng: &mut R, buffer: &mut Vec<u8>) {
    buffer.push(0xff); // cons
    write_int(buffer, op.opcode as u64);
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
                // quoted value
                buffer.push(0xff); // cons
                buffer.push(1); // quote
                generate(*arg, rng, buffer);
            } else {
                let sub_op = sample(rng, &potential_ops);
                generate_program(sub_op, rng, buffer);
            }
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
            write_int(buffer, *sample(rng, &INTERESTING_U64));
        }
        Type::Int32 => {
            write_int(buffer, *sample(rng, &INTERESTING_U32) as u64);
        }
        Type::Zero => {
            buffer.push(0x80);
        }
        Type::Cost => {
            write_atom(buffer, &8000000000_u64.to_be_bytes()).expect("write_atom failed");
        }
        Type::G1Point => {
            #[allow(clippy::borrow_deref_ref)]
            write_atom(buffer, &*sample(rng, &G1POINTS)).expect("write_atom failed");
        }
        Type::G2Point => {
            #[allow(clippy::borrow_deref_ref)]
            write_atom(buffer, &*sample(rng, &G2POINTS)).expect("write_atom failed");
        }
        Type::Bytes20 => {
            #[allow(clippy::borrow_deref_ref)]
            write_atom(buffer, &*sample(rng, &BYTES20)).expect("write_atom failed");
        }
        Type::Bytes32 => {
            #[allow(clippy::borrow_deref_ref)]
            #[allow(clippy::needless_borrow)]
            write_atom(buffer, &*sample(rng, &BYTES32)).expect("write_atom failed");
        }
        Type::Sec1 => {
            #[allow(clippy::borrow_deref_ref)]
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::explicit_auto_deref)]
            write_atom(buffer, &*sample(rng, &SEC1)).expect("write_atom failed");
        }
        Type::Sig => {
            #[allow(clippy::borrow_deref_ref)]
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::explicit_auto_deref)]
            write_atom(buffer, &*sample(rng, &SIG)).expect("write_atom failed");
        }
        Type::Bytes96 => {
            write_atom(buffer, &ZEROS[..96]).expect("write_atom failed");
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

    for i in 0..40000 {
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

    for i in 0..40000 {
        buffer.truncate(0);

        let op = &OPERATORS[i % OPERATORS.len()];
        generate_args(op, &mut rng, &mut buffer);
        let mut out = File::create(format!("../fuzz/corpus/operators/{}", filename(&buffer)))
            .expect("failed to open file");
        out.write_all(&buffer).expect("failed to write file");
    }
}
