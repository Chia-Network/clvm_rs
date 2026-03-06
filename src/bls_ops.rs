use crate::allocator::{Allocator, Atom, NodePtr};
use crate::cost::{Cost, check_cost};
use crate::error::EvalErr;
use crate::op_utils::{
    MALLOC_COST_PER_BYTE, atom, first, get_args, get_varargs, int_atom, mod_group_order,
    new_atom_and_cost, nilp, rest,
};
use crate::reduction::{Reduction, Response};
use chia_bls::{
    G1Element, G2Element, PublicKey, aggregate_pairing, aggregate_verify, hash_to_g1_with_dst,
    hash_to_g2_with_dst,
};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

static G1_CACHE: OnceLock<RwLock<HashMap<[u8; 48], bool>>> = OnceLock::new();
static G2_CACHE: OnceLock<RwLock<HashMap<[u8; 96], bool>>> = OnceLock::new();

// Evict when the cache exceeds this many entries to bound memory usage.
// Each entry stores both the point and its negation, so a 65536-entry cache
// holds up to ~32768 distinct validated points.
const MAX_CACHE_ENTRIES: usize = 65536;

/// Returns (negated_blob, is_infinity)
fn negate_g1_bytes(blob: &[u8; 48]) -> ([u8; 48], bool) {
    let mut neg = *blob;
    let is_inf = (neg[0] & 0xe0) == 0xc0;
    if !is_inf {
        neg[0] ^= 0x20;
    }
    (neg, is_inf)
}

fn negate_g2_bytes(blob: &[u8; 96]) -> ([u8; 96], bool) {
    let mut neg = *blob;
    let is_inf = (neg[0] & 0xe0) == 0xc0;
    if !is_inf {
        neg[0] ^= 0x20;
    }
    (neg, is_inf)
}

/// Check if a G1 point (compressed, 48 bytes) is valid.
/// Caches the result (and its negation) for future lookups.
fn g1_check_valid(blob: &[u8; 48]) -> bool {
    let cache = G1_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let (neg, _) = negate_g1_bytes(blob);

    {
        let r = cache.read().unwrap();
        if let Some(&v) = r.get(blob).or_else(|| r.get(&neg)) {
            return v;
        }
    }

    let valid = G1Element::from_bytes(blob).is_ok();
    let mut w = cache.write().unwrap();
    if w.len() >= MAX_CACHE_ENTRIES {
        w.clear();
    }
    w.insert(*blob, valid);
    w.insert(neg, valid);
    valid
}

fn g2_check_valid(blob: &[u8; 96]) -> bool {
    let cache = G2_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let (neg, _) = negate_g2_bytes(blob);

    {
        let r = cache.read().unwrap();
        if let Some(&v) = r.get(blob).or_else(|| r.get(&neg)) {
            return v;
        }
    }

    let valid = G2Element::from_bytes(blob).is_ok();
    let mut w = cache.write().unwrap();
    if w.len() >= MAX_CACHE_ENTRIES {
        w.clear();
    }
    w.insert(*blob, valid);
    w.insert(neg, valid);
    valid
}

// the same cost as point_add (aka g1_add)
const BLS_G1_SUBTRACT_BASE_COST: Cost = 101094;
const BLS_G1_SUBTRACT_COST_PER_ARG: Cost = 1343980;

const BLS_G1_MULTIPLY_BASE_COST: Cost = 705500;
const BLS_G1_MULTIPLY_COST_PER_BYTE: Cost = 10;

// this is the same cost as XORing the top bit (minus the heap allocation of the
// return value, which the operator is adding back)
const BLS_G1_NEGATE_BASE_COST: Cost = 1396 - 480;

// g2_add and g2_subtract have the same cost
const BLS_G2_ADD_BASE_COST: Cost = 80000;
const BLS_G2_ADD_COST_PER_ARG: Cost = 1950000;
const BLS_G2_SUBTRACT_BASE_COST: Cost = 80000;
const BLS_G2_SUBTRACT_COST_PER_ARG: Cost = 1950000;

const BLS_G2_MULTIPLY_BASE_COST: Cost = 2100000;
const BLS_G2_MULTIPLY_COST_PER_BYTE: Cost = 5;

// this is the same cost as XORing the top bit (minus the heap allocation of the
// return value, which the operator is adding back)
const BLS_G2_NEGATE_BASE_COST: Cost = 2164 - 960;

const BLS_MAP_TO_G1_BASE_COST: Cost = 195000;
const BLS_MAP_TO_G1_COST_PER_BYTE: Cost = 4;
const BLS_MAP_TO_G1_COST_PER_DST_BYTE: Cost = 4;

const BLS_MAP_TO_G2_BASE_COST: Cost = 815000;
const BLS_MAP_TO_G2_COST_PER_BYTE: Cost = 4;
const BLS_MAP_TO_G2_COST_PER_DST_BYTE: Cost = 4;

const BLS_PAIRING_BASE_COST: Cost = 3000000;
const BLS_PAIRING_COST_PER_ARG: Cost = 1200000;

const DST_G2: &[u8; 43] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_AUG_";

pub fn op_bls_g1_subtract(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BLS_G1_SUBTRACT_BASE_COST;
    check_cost(cost, max_cost)?;
    let mut total = G1Element::default();
    let mut is_first = true;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let point = a.g1(arg)?;
        cost += BLS_G1_SUBTRACT_COST_PER_ARG;
        check_cost(cost, max_cost)?;
        if is_first {
            total = point;
        } else {
            total -= &point;
        };
        is_first = false;
    }
    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(total)?,
    ))
}

pub fn op_bls_g1_multiply(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let [point, scalar] = get_args::<2>(a, input, "g1_multiply")?;

    let mut cost = BLS_G1_MULTIPLY_BASE_COST;
    check_cost(cost, max_cost)?;

    let mut total = a.g1(point)?;
    let (scalar, scalar_len) = int_atom(a, scalar, "g1_multiply")?;
    cost += scalar_len as Cost * BLS_G1_MULTIPLY_COST_PER_BYTE;
    check_cost(cost, max_cost)?;

    let scalar = mod_group_order(scalar);
    total.scalar_multiply(scalar.to_bytes_be().1.as_slice());

    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(total)?,
    ))
}

pub fn op_bls_g1_negate(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    op_bls_g1_negate_impl(a, input, false)
}

pub fn op_bls_g1_negate_strict(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    op_bls_g1_negate_impl(a, input, true)
}

fn op_bls_g1_negate_impl(a: &mut Allocator, input: NodePtr, strict: bool) -> Response {
    let [point] = get_args::<1>(a, input, "g1_negate")?;

    let blob = atom(a, point, "G1 atom")?;
    // this is here to validate the point
    if strict {
        let blob_array: [u8; 48] = blob.as_ref().try_into().map_err(|_| {
            EvalErr::InvalidOpArg(point, "atom is not a G1 size, 48 bytes".to_string())
        })?;
        if !g1_check_valid(&blob_array) {
            return Err(EvalErr::InvalidOpArg(
                point,
                "atom is not a G1 point".to_string(),
            ));
        }
    } else if blob.len() != 48 {
        return Err(EvalErr::InvalidOpArg(
            point,
            "atom is not G1 size, 48 bytes".to_string(),
        ));
    }

    if (blob.as_ref()[0] & 0xe0) == 0xc0 {
        // This is compressed infinity. negating it is a no-op
        // we can just pass through the same atom as we received. We'll charge
        // the allocation cost anyway, for consistency
        Ok(Reduction(
            BLS_G1_NEGATE_BASE_COST + 48 * MALLOC_COST_PER_BYTE,
            point,
        ))
    } else {
        let mut blob: [u8; 48] = blob.as_ref().try_into().unwrap();
        blob[0] ^= 0x20;
        new_atom_and_cost(a, BLS_G1_NEGATE_BASE_COST, &blob)
    }
}

pub fn op_bls_g2_add(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BLS_G2_ADD_BASE_COST;
    check_cost(cost, max_cost)?;
    let mut total = G2Element::default();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let point = a.g2(arg)?;
        cost += BLS_G2_ADD_COST_PER_ARG;
        check_cost(cost, max_cost)?;
        total += &point;
    }
    Ok(Reduction(
        cost + 96 * MALLOC_COST_PER_BYTE,
        a.new_g2(total)?,
    ))
}

pub fn op_bls_g2_subtract(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BLS_G2_SUBTRACT_BASE_COST;
    check_cost(cost, max_cost)?;
    let mut total = G2Element::default();
    let mut is_first = true;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let point = a.g2(arg)?;
        cost += BLS_G2_SUBTRACT_COST_PER_ARG;
        check_cost(cost, max_cost)?;
        if is_first {
            total = point;
        } else {
            total -= &point;
        };
        is_first = false;
    }
    Ok(Reduction(
        cost + 96 * MALLOC_COST_PER_BYTE,
        a.new_g2(total)?,
    ))
}

pub fn op_bls_g2_multiply(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let [point, scalar] = get_args::<2>(a, input, "g2_multiply")?;

    let mut cost = BLS_G2_MULTIPLY_BASE_COST;
    check_cost(cost, max_cost)?;

    let mut total = a.g2(point)?;
    let (scalar, scalar_len) = int_atom(a, scalar, "g2_multiply")?;
    cost += scalar_len as Cost * BLS_G2_MULTIPLY_COST_PER_BYTE;
    check_cost(cost, max_cost)?;

    let scalar = mod_group_order(scalar);
    total.scalar_multiply(scalar.to_bytes_be().1.as_slice());

    Ok(Reduction(
        cost + 96 * MALLOC_COST_PER_BYTE,
        a.new_g2(total)?,
    ))
}

pub fn op_bls_g2_negate(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    op_bls_g2_negate_impl(a, input, false)
}

pub fn op_bls_g2_negate_strict(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    op_bls_g2_negate_impl(a, input, true)
}

fn op_bls_g2_negate_impl(a: &mut Allocator, input: NodePtr, strict: bool) -> Response {
    let [point] = get_args::<1>(a, input, "g2_negate")?;

    // we don't validate the point. We may want to soft fork-in validating the
    // point once the allocator preserves native representation of points
    let blob_atom = atom(a, point, "G2 atom")?;
    let blob = blob_atom.as_ref();

    // this is here to validate the point
    if strict {
        let blob_array: [u8; 96] = blob.as_ref().try_into().map_err(|_| {
            EvalErr::InvalidOpArg(point, "atom is not G2 size, 96 bytes".to_string())
        })?;
        if !g2_check_valid(&blob_array) {
            return Err(EvalErr::InvalidOpArg(
                point,
                "atom is not a G2 point".to_string(),
            ));
        }
    } else if blob.len() != 96 {
        return Err(EvalErr::InvalidOpArg(
            point,
            "atom is not G2 size, 96 bytes".to_string(),
        ));
    }

    if (blob[0] & 0xe0) == 0xc0 {
        // This is compressed infinity. negating it is a no-op
        // we can just pass through the same atom as we received. We'll charge
        // the allocation cost anyway, for consistency
        Ok(Reduction(
            BLS_G2_NEGATE_BASE_COST + 96 * MALLOC_COST_PER_BYTE,
            point,
        ))
    } else {
        let mut blob: [u8; 96] = blob.as_ref().try_into().unwrap();
        blob[0] ^= 0x20;
        new_atom_and_cost(a, BLS_G2_NEGATE_BASE_COST, &blob)
    }
}

pub fn op_bls_map_to_g1(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let ([msg, dst], argc) = get_varargs::<2>(a, input, "g1_map")?;
    if !(1..=2).contains(&argc) {
        Err(EvalErr::InvalidOpArg(
            input,
            format!("g1_map takes exactly 1 or 2 arguments, got {argc}"),
        ))?;
    }
    let mut cost: Cost = BLS_MAP_TO_G1_BASE_COST;
    check_cost(cost, max_cost)?;

    let msg = atom(a, msg, "g1_map")?;
    cost += msg.as_ref().len() as Cost * BLS_MAP_TO_G1_COST_PER_BYTE;
    check_cost(cost, max_cost)?;

    let dst = if argc == 2 {
        atom(a, dst, "g1_map")?
    } else {
        Atom::Borrowed(b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_AUG_".as_slice())
    };

    cost += dst.as_ref().len() as Cost * BLS_MAP_TO_G1_COST_PER_DST_BYTE;
    check_cost(cost, max_cost)?;

    let point = hash_to_g1_with_dst(msg.as_ref(), dst.as_ref());
    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(point)?,
    ))
}

pub fn op_bls_map_to_g2(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let ([msg, dst], argc) = get_varargs::<2>(a, input, "g2_map")?;
    if !(1..=2).contains(&argc) {
        Err(EvalErr::InvalidOpArg(
            input,
            format!("g2_map takes exactly 1 or 2 arguments, got {argc}"),
        ))?;
    }
    let mut cost: Cost = BLS_MAP_TO_G2_BASE_COST;
    check_cost(cost, max_cost)?;

    let msg = atom(a, msg, "g2_map")?;
    cost += msg.as_ref().len() as Cost * BLS_MAP_TO_G2_COST_PER_BYTE;

    let dst = if argc == 2 {
        atom(a, dst, "g2_map")?
    } else {
        Atom::Borrowed(DST_G2.as_slice())
    };

    cost += dst.as_ref().len() as Cost * BLS_MAP_TO_G2_COST_PER_DST_BYTE;
    check_cost(cost, max_cost)?;

    let point = hash_to_g2_with_dst(msg.as_ref(), dst.as_ref());
    Ok(Reduction(
        cost + 96 * MALLOC_COST_PER_BYTE,
        a.new_g2(point)?,
    ))
}

// This operator takes a variable number of G1 and G2 points. The points must
// come in pairs (as a "flat" argument list).
// It performs a low-level pairing operation of the (G1, G2)-pairs
// and returns if the resulting Gt point is the
// identity, otherwise terminates the program with a validation error.
pub fn op_bls_pairing_identity(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BLS_PAIRING_BASE_COST;
    check_cost(cost, max_cost)?;
    let mut items = Vec::<(G1Element, G2Element)>::new();

    let mut args = input;
    while !nilp(a, args) {
        cost += BLS_PAIRING_COST_PER_ARG;
        check_cost(cost, max_cost)?;
        let g1 = a.g1(first(a, args)?)?;
        args = rest(a, args)?;
        let g2 = a.g2(first(a, args)?)?;
        args = rest(a, args)?;
        items.push((g1, g2));
    }

    if !aggregate_pairing(items) {
        Err(EvalErr::BLSPairingIdentityFailed(input))?
    } else {
        Ok(Reduction(cost, a.nil()))
    }
}

// expects: G2 G1 msg G1 msg ...
// G2 is the signature
// G1 is a public key
// the G1 and its corresponding message must be passed in pairs.
pub fn op_bls_verify(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BLS_PAIRING_BASE_COST;
    check_cost(cost, max_cost)?;

    let mut args = input;

    // the first argument is the signature
    let signature = a.g2(first(a, args)?)?;

    // followed by a variable number of (G1, msg)-pairs (as a flat list)
    args = rest(a, args)?;

    let mut items = Vec::<(PublicKey, Atom)>::new();
    while !nilp(a, args) {
        let pk = a.g1(first(a, args)?)?;
        args = rest(a, args)?;
        let msg = atom(a, first(a, args)?, "bls_verify message")?;
        args = rest(a, args)?;

        cost += BLS_PAIRING_COST_PER_ARG;
        cost += msg.as_ref().len() as Cost * BLS_MAP_TO_G2_COST_PER_BYTE;
        cost += DST_G2.len() as Cost * BLS_MAP_TO_G2_COST_PER_DST_BYTE;
        check_cost(cost, max_cost)?;

        items.push((pk, msg));
    }

    if !aggregate_verify(&signature, items) {
        Err(EvalErr::BLSVerifyFailed(input))?
    } else {
        Ok(Reduction(cost, a.nil()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocator::Allocator;

    #[test]
    fn test_g1_negate_strict_invalid_point_cached() {
        let mut a = Allocator::new();
        let invalid_g1 = a.new_atom(&hex::decode("b7f1d3a7319092346345638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb").unwrap()).unwrap();
        let input = a.new_pair(invalid_g1, a.nil()).unwrap();

        let result1 = op_bls_g1_negate_strict(&mut a, input, 1_000_000);
        assert!(result1.is_err());

        let result2 = op_bls_g1_negate_strict(&mut a, input, 1_000_000);
        assert!(result2.is_err());
    }

    #[test]
    fn test_g2_negate_strict_invalid_point_cached() {
        let mut a = Allocator::new();
        let invalid_g2 = a.new_atom(&hex::decode("b3e02b6052719f624359072893758937457903459920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8").unwrap()).unwrap();
        let input = a.new_pair(invalid_g2, a.nil()).unwrap();

        let result1 = op_bls_g2_negate_strict(&mut a, input, 10_000_000);
        assert!(result1.is_err());

        let result2 = op_bls_g2_negate_strict(&mut a, input, 10_000_000);
        assert!(result2.is_err());
    }
}
