use hex_literal::hex;
use num_bigint::{BigUint, Sign};
use num_integer::Integer;
use std::ops::BitAndAssign;
use std::ops::BitOrAssign;
use std::ops::BitXorAssign;

use crate::allocator::{len_for_value, Allocator, NodePtr, NodeVisitor, SExp};
use crate::cost::{check_cost, Cost};
use crate::err_utils::err;
use crate::number::Number;
use crate::op_utils::{
    atom, atom_len, get_args, get_varargs, i32_atom, int_atom, match_args, mod_group_order,
    new_atom_and_cost, nilp, u32_from_u8, MALLOC_COST_PER_BYTE,
};
use crate::reduction::{Reduction, Response};
use chia_bls::G1Element;
use chia_sha2::Sha256;

const ARITH_BASE_COST: Cost = 99;
const ARITH_COST_PER_ARG: Cost = 320;
const ARITH_COST_PER_BYTE: Cost = 3;

const LOG_BASE_COST: Cost = 100;
const LOG_COST_PER_ARG: Cost = 264;
const LOG_COST_PER_BYTE: Cost = 3;

const LOGNOT_BASE_COST: Cost = 331;
const LOGNOT_COST_PER_BYTE: Cost = 3;

const MUL_BASE_COST: Cost = 92;
const MUL_COST_PER_OP: Cost = 885;
const MUL_LINEAR_COST_PER_BYTE: Cost = 6;
const MUL_SQUARE_COST_PER_BYTE_DIVIDER: Cost = 128;

const GR_BASE_COST: Cost = 498;
const GR_COST_PER_BYTE: Cost = 2;

const GRS_BASE_COST: Cost = 117;
const GRS_COST_PER_BYTE: Cost = 1;

const STRLEN_BASE_COST: Cost = 173;
const STRLEN_COST_PER_BYTE: Cost = 1;

const CONCAT_BASE_COST: Cost = 142;
const CONCAT_COST_PER_ARG: Cost = 135;
const CONCAT_COST_PER_BYTE: Cost = 3;

const DIVMOD_BASE_COST: Cost = 1116;
const DIVMOD_COST_PER_BYTE: Cost = 6;

const DIV_BASE_COST: Cost = 988;
const DIV_COST_PER_BYTE: Cost = 4;

const SHA256_BASE_COST: Cost = 87;
const SHA256_COST_PER_ARG: Cost = 134;
const SHA256_COST_PER_BYTE: Cost = 2;

const ASHIFT_BASE_COST: Cost = 596;
const ASHIFT_COST_PER_BYTE: Cost = 3;

const LSHIFT_BASE_COST: Cost = 277;
const LSHIFT_COST_PER_BYTE: Cost = 3;

const BOOL_BASE_COST: Cost = 200;
const BOOL_COST_PER_ARG: Cost = 300;

// Raspberry PI 4 is about 7.679960 / 1.201742 = 6.39 times slower
// in the point_add benchmark

// increased from 31592 to better model Raspberry PI
const POINT_ADD_BASE_COST: Cost = 101094;
// increased from 419994 to better model Raspberry PI
const POINT_ADD_COST_PER_ARG: Cost = 1343980;

// Raspberry PI 4 is about 2.833543 / 0.447859 = 6.32686 times slower
// in the pubkey benchmark

// increased from 419535 to better model Raspberry PI
const PUBKEY_BASE_COST: Cost = 1325730;
// increased from 12 to closer model Raspberry PI
const PUBKEY_COST_PER_BYTE: Cost = 38;

// the new coinid operator
// we subtract 153 cost as a discount, to incentivize using this operator rather
// than "naked" sha256
const COINID_COST: Cost =
    SHA256_BASE_COST + SHA256_COST_PER_ARG * 3 + SHA256_COST_PER_BYTE * (32 + 32 + 8) - 153;

const MODPOW_BASE_COST: Cost = 17000;
const MODPOW_COST_PER_BYTE_BASE_VALUE: Cost = 38;
// the cost for exponent and modular scale by the square of the size of the
// respective operands
const MODPOW_COST_PER_BYTE_EXPONENT: Cost = 3;
const MODPOW_COST_PER_BYTE_MOD: Cost = 21;

fn limbs_for_int(v: &Number) -> usize {
    v.bits().div_ceil(8) as usize
}

#[cfg(test)]
fn limb_test_helper(bytes: &[u8]) {
    let bigint = Number::from_signed_bytes_be(bytes);
    println!("{} bits: {}", &bigint, &bigint.bits());

    // redundant leading zeros don't count, since they aren't stored internally
    let expected = if !bytes.is_empty() && bytes[0] == 0 {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    assert_eq!(limbs_for_int(&bigint), expected);
}

#[test]
fn test_limbs_for_int() {
    limb_test_helper(&[]);
    limb_test_helper(&[0x1]);
    limb_test_helper(&[0x80]);
    limb_test_helper(&[0x81]);
    limb_test_helper(&[0x7f]);
    limb_test_helper(&[0xff]);
    limb_test_helper(&[0, 0xff]);
    limb_test_helper(&[0x7f, 0xff]);
    limb_test_helper(&[0x7f, 0]);
    limb_test_helper(&[0x7f, 0x77]);

    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0]);

    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x20, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x10, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x08, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x04, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x02, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x01, 0, 0, 0, 0, 0, 0]);

    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0]);
}

fn malloc_cost(a: &Allocator, cost: Cost, ptr: NodePtr) -> Reduction {
    let c = a.atom_len(ptr) as Cost * MALLOC_COST_PER_BYTE;
    Reduction(cost + c, ptr)
}

pub fn op_unknown(
    allocator: &mut Allocator,
    o: NodePtr,
    mut args: NodePtr,
    max_cost: Cost,
) -> Response {
    // unknown opcode in lenient mode
    // unknown ops are reserved if they start with 0xffff
    // otherwise, unknown ops are no-ops, but they have costs. The cost is computed
    // like this:

    // byte index (reverse):
    // | 4 | 3 | 2 | 1 | 0          |
    // +---+---+---+---+------------+
    // | multiplier    |XX | XXXXXX |
    // +---+---+---+---+---+--------+
    //  ^               ^    ^
    //  |               |    + 6 bits ignored when computing cost
    // cost_multiplier  |
    // (up to 4 bytes)  + 2 bits
    //                    cost_function

    // 1 is always added to the multiplier before using it to multiply the cost, this
    // is since cost may not be 0.

    // cost_function is 2 bits and defines how cost is computed based on arguments:
    // 0: constant, cost is 1 * (multiplier + 1)
    // 1: computed like operator add, multiplied by (multiplier + 1)
    // 2: computed like operator mul, multiplied by (multiplier + 1)
    // 3: computed like operator concat, multiplied by (multiplier + 1)

    // this means that unknown ops where cost_function is 1, 2, or 3, may still be
    // fatal errors if the arguments passed are not atoms.

    let op_atom = allocator.atom(o);
    let op = op_atom.as_ref();

    if op.is_empty() || (op.len() >= 2 && op[0] == 0xff && op[1] == 0xff) {
        return err(o, "reserved operator");
    }

    let cost_function = (op[op.len() - 1] & 0b11000000) >> 6;
    let cost_multiplier: u64 = match u32_from_u8(&op[0..op.len() - 1]) {
        Some(v) => v as u64,
        None => {
            return err(o, "invalid operator");
        }
    };

    let mut cost = match cost_function {
        0 => 1,
        1 => {
            let mut cost = ARITH_BASE_COST;
            let mut byte_count: u64 = 0;
            while let Some((arg, rest)) = allocator.next(args) {
                args = rest;
                cost += ARITH_COST_PER_ARG;
                let len = atom_len(allocator, arg, "unknown op")?;
                byte_count += len as u64;
                check_cost(
                    allocator,
                    cost + (byte_count as Cost * ARITH_COST_PER_BYTE),
                    max_cost,
                )?;
            }
            cost + (byte_count * ARITH_COST_PER_BYTE)
        }
        2 => {
            let mut cost = MUL_BASE_COST;
            let mut first_iter: bool = true;
            let mut l0: u64 = 0;
            while let Some((arg, rest)) = allocator.next(args) {
                args = rest;
                let len = atom_len(allocator, arg, "unknown op")?;
                if first_iter {
                    l0 = len as u64;
                    first_iter = false;
                    continue;
                }
                let l1 = len as u64;
                cost += MUL_COST_PER_OP;
                cost += (l0 + l1) * MUL_LINEAR_COST_PER_BYTE;
                cost += (l0 * l1) / MUL_SQUARE_COST_PER_BYTE_DIVIDER;
                l0 += l1;
                check_cost(allocator, cost, max_cost)?;
            }
            cost
        }
        3 => {
            let mut cost = CONCAT_BASE_COST;
            let mut total_size: u64 = 0;
            while let Some((arg, rest)) = allocator.next(args) {
                args = rest;
                cost += CONCAT_COST_PER_ARG;
                let len = atom_len(allocator, arg, "unknown op")?;
                total_size += len as u64;
                check_cost(
                    allocator,
                    cost + total_size as Cost * CONCAT_COST_PER_BYTE,
                    max_cost,
                )?;
            }
            cost + total_size * CONCAT_COST_PER_BYTE
        }
        _ => 1,
    };

    assert!(cost > 0);

    check_cost(allocator, cost, max_cost)?;
    cost *= cost_multiplier + 1;
    if cost > u32::MAX as u64 {
        err(o, "invalid operator")
    } else {
        Ok(Reduction(cost as Cost, allocator.nil()))
    }
}

#[cfg(test)]
fn test_op_unknown(buf: &[u8], a: &mut Allocator, n: NodePtr) -> Response {
    let buf = a.new_atom(buf)?;
    op_unknown(a, buf, n, 1000000)
}

#[test]
fn test_unknown_op_reserved() {
    let mut a = Allocator::new();

    // any op starting with ffff is reserved and a hard failure
    let buf = vec![0xff, 0xff];
    let nil = a.nil();
    assert!(test_op_unknown(&buf, &mut a, nil).is_err());

    let buf = vec![0xff, 0xff, 0xff];
    assert!(test_op_unknown(&buf, &mut a, nil).is_err());

    let buf = vec![0xff, 0xff, b'0'];
    assert!(test_op_unknown(&buf, &mut a, nil).is_err());

    let buf = vec![0xff, 0xff, 0];
    assert!(test_op_unknown(&buf, &mut a, nil).is_err());

    let buf = vec![0xff, 0xff, 0xcc, 0xcc, 0xfe, 0xed, 0xce];
    assert!(test_op_unknown(&buf, &mut a, nil).is_err());

    // an empty atom is not a valid opcode
    let buf = Vec::<u8>::new();
    assert!(test_op_unknown(&buf, &mut a, nil).is_err());

    // a single ff is not sufficient to be treated as a reserved opcode
    let buf = vec![0xff];
    assert_eq!(test_op_unknown(&buf, &mut a, nil), Ok(Reduction(142, nil)));

    // leading zeros count, so this is not considered an ffff-prefix
    let buf = vec![0x00, 0xff, 0xff, 0x00, 0x00];
    // the cost is 0xffff00 = 16776960 plus the implied 1
    assert_eq!(
        test_op_unknown(&buf, &mut a, nil),
        Ok(Reduction(16776961, nil))
    );
}

#[test]
fn test_lenient_mode_last_bits() {
    let mut a = crate::allocator::Allocator::new();

    // the last 6 bits are ignored for computing cost
    let buf = vec![0x3c, 0x3f];
    let nil = a.nil();
    assert_eq!(test_op_unknown(&buf, &mut a, nil), Ok(Reduction(61, nil)));

    let buf = vec![0x3c, 0x0f];
    assert_eq!(test_op_unknown(&buf, &mut a, nil), Ok(Reduction(61, nil)));

    let buf = vec![0x3c, 0x00];
    assert_eq!(test_op_unknown(&buf, &mut a, nil), Ok(Reduction(61, nil)));

    let buf = vec![0x3c, 0x2c];
    assert_eq!(test_op_unknown(&buf, &mut a, nil), Ok(Reduction(61, nil)));
}

// contains SHA256(1 .. x), where x is the index into the array and .. is
// concatenation. This was computed by:
// print(f"    hex!(\"{sha256(bytes([1])).hexdigest()}\"),")
// for i in range(1, 37):
//     print(f"    hex!(\"{sha256(bytes([1, i])).hexdigest()}\"),")
pub const PRECOMPUTED_HASHES: [[u8; 32]; 37] = [
    hex!("4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"),
    hex!("9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2"),
    hex!("a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222"),
    hex!("c79b932e1e1da3c0e098e5ad2c422937eb904a76cf61d83975a74a68fbb04b99"),
    hex!("a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5"),
    hex!("bc5959f43bc6e47175374b6716e53c9a7d72c59424c821336995bad760d9aeb3"),
    hex!("44602a999abbebedf7de0ae1318e4f57e3cb1d67e482a65f9657f7541f3fe4bb"),
    hex!("ca6c6588fa01171b200740344d354e8548b7470061fb32a34f4feee470ec281f"),
    hex!("9e6282e4f25e370ce617e21d6fe265e88b9e7b8682cf00059b9d128d9381f09d"),
    hex!("ac9e61d54eb6967e212c06aab15408292f8558c48f06f9d705150063c68753b0"),
    hex!("c04b5bb1a5b2eb3e9cd4805420dba5a9d133da5b7adeeafb5474c4adae9faa80"),
    hex!("57bfd1cb0adda3d94315053fda723f2028320faa8338225d99f629e3d46d43a9"),
    hex!("6b6daa8334bbcc8f6b5906b6c04be041d92700b74024f73f50e0a9f0dae5f06f"),
    hex!("c7b89cfb9abf2c4cb212a4840b37d762f4c880b8517b0dadb0c310ded24dd86d"),
    hex!("653b3bb3e18ef84d5b1e8ff9884aecf1950c7a1c98715411c22b987663b86dda"),
    hex!("24255ef5d941493b9978f3aabb0ed07d084ade196d23f463ff058954cbf6e9b6"),
    hex!("af340aa58ea7d72c2f9a7405f3734167bb27dd2a520d216addef65f8362102b6"),
    hex!("26e7f98cfafee5b213726e22632923bf31bf3e988233235f8f5ca5466b3ac0ed"),
    hex!("115b498ce94335826baa16386cd1e2fde8ca408f6f50f3785964f263cdf37ebe"),
    hex!("d8c50d6282a1ba47f0a23430d177bbfbb72e2b84713745e894f575570f1f3d6e"),
    hex!("dbe726e81a7221a385e007ef9e834a975a4b528c6f55a5d2ece288bee831a3d1"),
    hex!("764c8a3561c7cf261771b4e1969b84c210836f3c034baebac5e49a394a6ee0a9"),
    hex!("dce37f3512b6337d27290436ba9289e2fd6c775494c33668dd177cf811fbd47a"),
    hex!("5809addc9f6926fc5c4e20cf87958858c4454c21cdfc6b02f377f12c06b35cca"),
    hex!("b519be874447e0f0a38ee8ec84ecd2198a9fac778fccce19cc8d87be5d8ed6b1"),
    hex!("ae58b7e08e266680e93e46639a2a7e89fde78a6f3c8e4219d1087c406c25c24c"),
    hex!("2986113d3bc27183978188edd7e72c3352c5cd6c8f2de6b65a466fc15bc2b49e"),
    hex!("145bfb83f7b3ef33ac1eada788c187e4d1feb7326bcf340bb060a62e75434854"),
    hex!("387da93c57e24aca43495b2e241399d532048e038ee0ed9ca740c22a06cbce91"),
    hex!("af2c6f1512d1cabedeaf129e0643863c5741973283e065564f2c00bde7c92fe1"),
    hex!("5df7504bc193ee4c3deadede1459eccca172e87cca35e81f11ce14be5e94acaf"),
    hex!("5d9ae980408df9325fbc46da2612c599ef76949450516ae38bf3b4c64721613d"),
    hex!("3145e7a95720a1db303f2198e796ea848f52b6079b5bf4f47d32fad69c2bce77"),
    hex!("c846f87c9d6bdfaa33038ac78269cfd5a08aa89a0918e99c4c4ae2e804a4f9a3"),
    hex!("b70654fead634e1ede4518ef34872c9d4f083a53773bdbfb75ae926bd3a4ce47"),
    hex!("b71de80778f2783383f5d5a3028af84eab2f18a4eb38968172ca41724dd4b3f4"),
    hex!("3f2d2a889d22530bd1abdc40ff1cbb23ca53ae3f1983e58c70d46a15c120e780"),
];

pub fn op_sha256(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = SHA256_BASE_COST;

    if let Some([v0, v1]) = match_args::<2>(a, input) {
        if a.small_number(v0) == Some(1) {
            if let Some(val) = a.small_number(v1) {
                // in this case, we're hashing 1 concatenated with a small
                // integer, we may have a pre-computed hash for this
                if (val as usize) < PRECOMPUTED_HASHES.len() {
                    let num_bytes = if val > 0 { 2 } else { 1 };
                    cost += num_bytes * SHA256_COST_PER_BYTE + 2 as Cost * SHA256_COST_PER_ARG;
                    return new_atom_and_cost(a, cost, &PRECOMPUTED_HASHES[val as usize]);
                }
            }
        }
    }

    let mut byte_count: usize = 0;
    let mut hasher = Sha256::new();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += SHA256_COST_PER_ARG;
        check_cost(
            a,
            cost + byte_count as Cost * SHA256_COST_PER_BYTE,
            max_cost,
        )?;
        let blob = atom(a, arg, "sha256")?;
        byte_count += blob.as_ref().len();
        hasher.update(blob);
    }
    cost += byte_count as Cost * SHA256_COST_PER_BYTE;
    new_atom_and_cost(a, cost, &hasher.finalize())
}

pub fn op_add(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += ARITH_COST_PER_ARG;
        check_cost(
            a,
            cost + (byte_count as Cost * ARITH_COST_PER_BYTE),
            max_cost,
        )?;

        match a.node(arg) {
            NodeVisitor::Buffer(buf) => {
                use crate::number::number_from_u8;
                total += number_from_u8(buf);
                byte_count += buf.len();
            }
            NodeVisitor::U32(val) => {
                total += val;
                byte_count += len_for_value(val);
            }
            NodeVisitor::Pair(_, _) => {
                return err(arg, "+ requires int args");
            }
        }
    }
    let total = a.new_number(total)?;
    cost += byte_count as Cost * ARITH_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_subtract(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    let mut is_first = true;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += ARITH_COST_PER_ARG;
        check_cost(a, cost + byte_count as Cost * ARITH_COST_PER_BYTE, max_cost)?;
        if is_first {
            let (v, len) = int_atom(a, arg, "-")?;
            byte_count = len;
            total = v;
        } else {
            match a.node(arg) {
                NodeVisitor::Buffer(buf) => {
                    use crate::number::number_from_u8;
                    total -= number_from_u8(buf);
                    byte_count += buf.len();
                }
                NodeVisitor::U32(val) => {
                    total -= val;
                    byte_count += len_for_value(val);
                }
                NodeVisitor::Pair(_, _) => {
                    return err(arg, "- requires int args");
                }
            }
        };
        is_first = false;
    }
    let total = a.new_number(total)?;
    cost += byte_count as Cost * ARITH_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_multiply(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost: Cost = MUL_BASE_COST;
    let mut first_iter: bool = true;
    let mut total: Number = 1.into();
    let mut l0: usize = 0;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        check_cost(a, cost, max_cost)?;
        if first_iter {
            (total, l0) = int_atom(a, arg, "*")?;
            first_iter = false;
            continue;
        }

        let l1 = match a.node(arg) {
            NodeVisitor::Buffer(buf) => {
                use crate::number::number_from_u8;
                total *= number_from_u8(buf);
                buf.len()
            }
            NodeVisitor::U32(val) => {
                total *= val;
                len_for_value(val)
            }
            NodeVisitor::Pair(_, _) => {
                return err(arg, "* requires int args");
            }
        };

        cost += MUL_COST_PER_OP;
        cost += (l0 + l1) as Cost * MUL_LINEAR_COST_PER_BYTE;
        cost += (l0 * l1) as Cost / MUL_SQUARE_COST_PER_BYTE_DIVIDER;
        l0 = limbs_for_int(&total);
    }
    let total = a.new_number(total)?;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_div(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, "/")?;
    let (a0, a0_len) = int_atom(a, v0, "/")?;
    let (a1, a1_len) = int_atom(a, v1, "/")?;
    let cost = DIV_BASE_COST + ((a0_len + a1_len) as Cost) * DIV_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        err(input, "div with 0")
    } else {
        let q = a0.div_floor(&a1);
        let q = a.new_number(q)?;
        Ok(malloc_cost(a, cost, q))
    }
}

pub fn op_divmod(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, "divmod")?;
    let (a0, a0_len) = int_atom(a, v0, "divmod")?;
    let (a1, a1_len) = int_atom(a, v1, "divmod")?;
    let cost = DIVMOD_BASE_COST + ((a0_len + a1_len) as Cost) * DIVMOD_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        err(input, "divmod with 0")
    } else {
        let (q, r) = a0.div_mod_floor(&a1);
        let q1 = a.new_number(q)?;
        let r1 = a.new_number(r)?;

        let c = (a.atom_len(q1) + a.atom_len(r1)) as Cost * MALLOC_COST_PER_BYTE;
        let r: NodePtr = a.new_pair(q1, r1)?;
        Ok(Reduction(cost + c, r))
    }
}

pub fn op_mod(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, "mod")?;
    let (a0, a0_len) = int_atom(a, v0, "mod")?;
    let (a1, a1_len) = int_atom(a, v1, "mod")?;
    let cost = DIV_BASE_COST + ((a0_len + a1_len) as Cost) * DIV_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        err(input, "mod with 0")
    } else {
        let q = a.new_number(a0.mod_floor(&a1))?;
        let c = a.atom_len(q) as Cost * MALLOC_COST_PER_BYTE;
        Ok(Reduction(cost + c, q))
    }
}

pub fn op_gr(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, ">")?;

    match (a.small_number(v0), a.small_number(v1)) {
        (Some(lhs), Some(rhs)) => {
            let cost =
                GR_BASE_COST + (len_for_value(lhs) + len_for_value(rhs)) as Cost * GR_COST_PER_BYTE;
            Ok(Reduction(cost, if lhs > rhs { a.one() } else { a.nil() }))
        }
        _ => {
            let (v0, v0_len) = int_atom(a, v0, ">")?;
            let (v1, v1_len) = int_atom(a, v1, ">")?;
            let cost = GR_BASE_COST + (v0_len + v1_len) as Cost * GR_COST_PER_BYTE;
            Ok(Reduction(cost, if v0 > v1 { a.one() } else { a.nil() }))
        }
    }
}

pub fn op_gr_bytes(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n0, n1] = get_args::<2>(a, input, ">s")?;
    let v0_atom = atom(a, n0, ">s")?;
    let v1_atom = atom(a, n1, ">s")?;
    let v0 = v0_atom.as_ref();
    let v1 = v1_atom.as_ref();
    let cost = GRS_BASE_COST + (v0.len() + v1.len()) as Cost * GRS_COST_PER_BYTE;
    Ok(Reduction(cost, if v0 > v1 { a.one() } else { a.nil() }))
}

pub fn op_strlen(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "strlen")?;
    let size = atom_len(a, n, "strlen")?;
    let size_node = a.new_number(size.into())?;
    let cost = STRLEN_BASE_COST + size as Cost * STRLEN_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, size_node))
}

pub fn op_substr(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let ([a0, start, end], argc) = get_varargs::<3>(a, input, "substr")?;
    if !(2..=3).contains(&argc) {
        return err(input, "substr takes exactly 2 or 3 arguments");
    }
    let size = atom_len(a, a0, "substr")?;
    let start = i32_atom(a, start, "substr")?;

    let end = if argc == 3 {
        i32_atom(a, end, "substr")?
    } else {
        size as i32
    };
    if end < 0 || start < 0 || end as usize > size || end < start {
        err(input, "invalid indices for substr")
    } else {
        let r = a.new_substr(a0, start as u32, end as u32)?;
        let cost: Cost = 1;
        Ok(Reduction(cost, r))
    }
}

pub fn op_concat(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = CONCAT_BASE_COST;
    let mut total_size: usize = 0;
    let mut terms = Vec::<NodePtr>::new();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += CONCAT_COST_PER_ARG;
        check_cost(
            a,
            cost + total_size as Cost * CONCAT_COST_PER_BYTE,
            max_cost,
        )?;
        match a.sexp(arg) {
            SExp::Pair(_, _) => return err(arg, "concat on list"),
            SExp::Atom => total_size += a.atom_len(arg),
        };
        terms.push(arg);
    }

    cost += total_size as Cost * CONCAT_COST_PER_BYTE;
    cost += total_size as Cost * MALLOC_COST_PER_BYTE;
    check_cost(a, cost, max_cost)?;
    let new_atom = a.new_concat(total_size, &terms)?;
    Ok(Reduction(cost, new_atom))
}

pub fn op_ash(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n0, n1] = get_args::<2>(a, input, "ash")?;
    let (i0, l0) = int_atom(a, n0, "ash")?;
    let a1 = i32_atom(a, n1, "ash")?;
    if !(-65535..=65535).contains(&a1) {
        return err(n1, "shift too large");
    }

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l1 = limbs_for_int(&v);
    let r = a.new_number(v)?;
    let cost = ASHIFT_BASE_COST + ((l0 + l1) as Cost) * ASHIFT_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, r))
}

#[cfg(test)]
fn test_shift(
    op: fn(&mut Allocator, NodePtr, Cost) -> Response,
    a: &mut Allocator,
    a1: &[u8],
    a2: &[u8],
) -> Response {
    let args = a.nil();
    let a2 = a.new_atom(a2).unwrap();
    let args = a.new_pair(a2, args).unwrap();
    let a1 = a.new_atom(a1).unwrap();
    let args = a.new_pair(a1, args).unwrap();
    op(a, args, 10000000 as Cost)
}

#[test]
fn test_op_ash() {
    let mut a = Allocator::new();

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x80, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x80, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_ash, &mut a, &[1], &[0x80, 0]).unwrap().1;
    assert_eq!(a.atom(node).as_ref(), &[]);

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x7f, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x7f, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_ash, &mut a, &[1], &[0x7f, 0]).unwrap().1;
    // the result is 1 followed by 4064 zeroes
    let node_atom = a.atom(node);
    let node_bytes = node_atom.as_ref();
    assert_eq!(node_bytes[0], 1);
    assert_eq!(node_bytes.len(), 4065);
}

pub fn op_lsh(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n0, n1] = get_args::<2>(a, input, "lsh")?;
    let b0_atom = atom(a, n0, "lsh")?;
    let b0 = b0_atom.as_ref();
    let a1 = i32_atom(a, n1, "lsh")?;
    if !(-65535..=65535).contains(&a1) {
        return err(n1, "shift too large");
    }
    let i0 = BigUint::from_bytes_be(b0);
    let l0 = b0.len();
    let i0: Number = i0.into();

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };

    let l1 = limbs_for_int(&v);
    let r = a.new_number(v)?;
    let cost = LSHIFT_BASE_COST + ((l0 + l1) as Cost) * LSHIFT_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, r))
}

#[test]
fn test_op_lsh() {
    let mut a = Allocator::new();

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x80, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x80, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_lsh, &mut a, &[1], &[0x80, 0]).unwrap().1;
    assert_eq!(a.atom(node).as_ref(), &[]);

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x7f, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x7f, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_lsh, &mut a, &[1], &[0x7f, 0]).unwrap().1;
    // the result is 1 followed by 4064 zeroes
    let node_atom = a.atom(node);
    let node_bytes = node_atom.as_ref();
    assert_eq!(node_bytes[0], 1);
    assert_eq!(node_bytes.len(), 4065);
}

fn binop_reduction(
    op_name: &str,
    a: &mut Allocator,
    initial_value: Number,
    mut input: NodePtr,
    max_cost: Cost,
    op_f: fn(&mut Number, &Number) -> (),
) -> Response {
    let mut total = initial_value;
    let mut arg_size: usize = 0;
    let mut cost = LOG_BASE_COST;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let (n0, len) = int_atom(a, arg, op_name)?;
        op_f(&mut total, &n0);
        arg_size += len;
        cost += LOG_COST_PER_ARG;
        check_cost(a, cost + (arg_size as Cost * LOG_COST_PER_BYTE), max_cost)?;
    }
    cost += arg_size as Cost * LOG_COST_PER_BYTE;
    let total = a.new_number(total)?;
    Ok(malloc_cost(a, cost, total))
}

fn logand_op(a: &mut Number, b: &Number) {
    a.bitand_assign(b);
}

pub fn op_logand(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let v: Number = (-1).into();
    binop_reduction("logand", a, v, input, max_cost, logand_op)
}

fn logior_op(a: &mut Number, b: &Number) {
    a.bitor_assign(b);
}

pub fn op_logior(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let v: Number = (0).into();
    binop_reduction("logior", a, v, input, max_cost, logior_op)
}

fn logxor_op(a: &mut Number, b: &Number) {
    a.bitxor_assign(b);
}

pub fn op_logxor(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let v: Number = (0).into();
    binop_reduction("logxor", a, v, input, max_cost, logxor_op)
}

pub fn op_lognot(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "lognot")?;
    let (mut n, len) = int_atom(a, n, "lognot")?;
    n = !n;
    let cost = LOGNOT_BASE_COST + ((len as Cost) * LOGNOT_COST_PER_BYTE);
    let r = a.new_number(n)?;
    Ok(malloc_cost(a, cost, r))
}

pub fn op_not(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "not")?;
    let r = if nilp(a, n) { a.one() } else { a.nil() };
    let cost = BOOL_BASE_COST;
    Ok(Reduction(cost, r))
}

pub fn op_any(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BOOL_BASE_COST;
    let mut is_any = false;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_any = is_any || !nilp(a, arg);
    }
    Ok(Reduction(cost, if is_any { a.one() } else { a.nil() }))
}

pub fn op_all(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BOOL_BASE_COST;
    let mut is_all = true;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_all = is_all && !nilp(a, arg);
    }
    Ok(Reduction(cost, if is_all { a.one() } else { a.nil() }))
}

pub fn op_pubkey_for_exp(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "pubkey_for_exp")?;
    let (v0, v0_len) = int_atom(a, n, "pubkey_for_exp")?;
    let bytes = mod_group_order(v0).to_bytes_be().1;

    let point = G1Element::from_integer(&bytes);

    let cost = PUBKEY_BASE_COST + (v0_len as Cost) * PUBKEY_COST_PER_BYTE;
    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(point)?,
    ))
}

pub fn op_point_add(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = POINT_ADD_BASE_COST;
    let mut total = G1Element::default();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let point = a.g1(arg)?;
        cost += POINT_ADD_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        total += &point;
    }
    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(total)?,
    ))
}

pub fn op_coinid(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [parent_coin, puzzle_hash, amount] = get_args::<3>(a, input, "coinid")?;

    let parent_coin = atom(a, parent_coin, "coinid")?;
    if parent_coin.as_ref().len() != 32 {
        return err(input, "coinid: invalid parent coin id (must be 32 bytes)");
    }
    let puzzle_hash = atom(a, puzzle_hash, "coinid")?;
    if puzzle_hash.as_ref().len() != 32 {
        return err(input, "coinid: invalid puzzle hash (must be 32 bytes)");
    }
    let amount_atom = atom(a, amount, "coinid")?;
    let amount = amount_atom.as_ref();
    if !amount.is_empty() {
        if (amount[0] & 0x80) != 0 {
            return err(input, "coinid: invalid amount (may not be negative");
        }
        if amount == [0_u8] || (amount.len() > 1 && amount[0] == 0 && (amount[1] & 0x80) == 0) {
            return err(
                input,
                "coinid: invalid amount (may not have redundant leading zero)",
            );
        }
        // the only valid coin value that's 9 bytes is when a leading zero is
        // required to not have the value interpreted as negative
        if amount.len() > 9 || (amount.len() == 9 && amount[0] != 0) {
            return err(
                input,
                "coinid: invalid amount (may not exceed max coin amount)",
            );
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(parent_coin);
    hasher.update(puzzle_hash);
    hasher.update(amount);
    let ret: [u8; 32] = hasher
        .finalize()
        .as_slice()
        .try_into()
        .expect("sha256 hash is not 32 bytes");

    new_atom_and_cost(a, COINID_COST, &ret)
}

pub fn op_modpow(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let [base, exponent, modulus] = get_args::<3>(a, input, "modpow")?;

    let mut cost = MODPOW_BASE_COST;
    let (base, bsize) = int_atom(a, base, "modpow")?;
    cost += bsize as Cost * MODPOW_COST_PER_BYTE_BASE_VALUE;
    let (exponent, esize) = int_atom(a, exponent, "modpow")?;
    cost += (esize * esize) as Cost * MODPOW_COST_PER_BYTE_EXPONENT;
    check_cost(a, cost, max_cost)?;
    let (modulus, msize) = int_atom(a, modulus, "modpow")?;
    cost += (msize * msize) as Cost * MODPOW_COST_PER_BYTE_MOD;
    check_cost(a, cost, max_cost)?;

    if exponent.sign() == Sign::Minus {
        return err(input, "modpow with negative exponent");
    }

    if modulus.sign() == Sign::NoSign {
        return err(input, "modpow with 0 modulus");
    }

    let ret = base.modpow(&exponent, &modulus);
    let ret = a.new_number(ret)?;
    Ok(malloc_cost(a, cost, ret))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_sha256_atom(buf: &[u8]) {
        let mut a = Allocator::new();
        let mut args = a.nil();
        let v = a.new_atom(buf).unwrap();
        args = a.new_pair(v, args).unwrap();
        let v = a.new_small_number(1).unwrap();
        args = a.new_pair(v, args).unwrap();

        let cost = SHA256_BASE_COST
            + (2 * SHA256_COST_PER_ARG)
            + ((1 + buf.len()) as Cost * SHA256_COST_PER_BYTE)
            + 32 * MALLOC_COST_PER_BYTE;
        let Reduction(actual_cost, result) = op_sha256(&mut a, args, cost).unwrap();

        let mut hasher = Sha256::new();
        hasher.update([1_u8]);
        if !buf.is_empty() {
            hasher.update(buf);
        }

        println!("buf: {buf:?}");
        assert_eq!(a.atom(result).as_ref(), hasher.finalize().as_slice());
        assert_eq!(actual_cost, cost);
    }

    #[test]
    fn sha256_small_values() {
        test_sha256_atom(&[]);
        for val in 0..255 {
            test_sha256_atom(&[val]);
        }

        for val in 0..255 {
            test_sha256_atom(&[0, val]);
        }

        for val in 0..255 {
            test_sha256_atom(&[0xff, val]);
        }
    }
}
