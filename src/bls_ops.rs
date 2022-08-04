use bls12_381::{multi_miller_loop, G1Affine, G1Projective, G2Affine, G2Prepared, G2Projective, Gt};
use bls12_381::hash_to_curve::{ExpandMsgXmd, HashToCurve};
use crate::allocator::{Allocator, NodePtr, SExp};
use crate::cost::{check_cost, Cost};
use crate::reduction::EvalErr;
use crate::node::Node;
use crate::number::number_from_u8;
use crate::op_utils::{
    arg_count, atom, check_arg_count, int_atom, new_atom_and_cost, number_to_scalar, mod_group_order
};
use crate::reduction::Response;
use std::convert::TryFrom;

// TODO get cost models
const BLS_G1_SUBTRACT_BASE_COST: Cost = 132332;
const BLS_G1_SUBTRACT_COST_PER_ARG: Cost = 1362553;
const BLS_G1_MULTIPLY_BASE_COST: Cost = 2154347;
const BLS_G1_MULTIPLY_COST_PER_BYTE: Cost = 12;
const BLS_G1_NEGATE_BASE_COST: Cost = 470779;
const BLS_G2_ADD_BASE_COST: Cost = 45440;
const BLS_G2_ADD_COST_PER_ARG: Cost = 5544581;
const BLS_G2_SUBTRACT_BASE_COST: Cost = 146290;
const BLS_G2_SUBTRACT_COST_PER_ARG: Cost = 5495272;
const BLS_G2_MULTIPLY_BASE_COST: Cost = 101094;
const BLS_G2_MULTIPLY_COST_PER_ARG: Cost = 1343980;
const BLS_G2_NEGATE_BASE_COST: Cost = 1893189;
const BLS_G2_NEGATE_COST_PER_ARG: Cost = 1343980;
const BLS_GT_ADD_BASE_COST: Cost = 101094;
const BLS_GT_ADD_COST_PER_ARG: Cost = 1343980;
const BLS_GT_SUBTRACT_BASE_COST: Cost = 101094;
const BLS_GT_SUBTRACT_COST_PER_ARG: Cost = 1343980;
const BLS_GT_MULTIPLY_BASE_COST: Cost = 101094;
const BLS_GT_MULTIPLY_COST_PER_ARG: Cost = 1343980;
const BLS_GT_NEGATE_BASE_COST: Cost = 101094;
const BLS_GT_NEGATE_COST_PER_ARG: Cost = 1343980;
const BLS_PAIRING_BASE_COST: Cost = 101094;
const BLS_PAIRING_COST_PER_ARG: Cost = 1343980;
const BLS_MAP_TO_G1_BASE_COST: Cost = 87;
const BLS_MAP_TO_G1_COST_PER_BYTE: Cost = 2;
const BLS_MAP_TO_G2_BASE_COST: Cost = 87;
const BLS_MAP_TO_G2_COST_PER_BYTE: Cost = 2;

fn g1_atom(node: &Node) -> Result<G1Affine, EvalErr> {
    let atom = node.atom();
    if atom.is_some().into() {
        let blob = atom.unwrap();
        if blob.len() == 48 {
            let mut as_array: [u8; 48] = [0; 48];
            as_array.clone_from_slice(&blob[0..48]);
            let v = G1Affine::from_compressed(&as_array);
            if v.is_some().into() {
                Ok(v.unwrap())
            } else {
                node.err("atom is not a G1 point")
            }
        } else {
            node.err(&format!("atom is not G1 size, got {}: Length of bytes object not equal to 48", hex::encode(blob)))
        }
    } else {
        node.err("G1 point is not an atom")
    }
}

fn g2_atom(node: &Node) -> Result<G2Affine, EvalErr> {
    let atom = node.atom();
    if atom.is_some().into() {
        let blob = atom.unwrap();
        if blob.len() == 96 {
            let mut as_array: [u8; 96] = [0; 96];
            as_array.clone_from_slice(&blob[0..96]);
            let v = G2Affine::from_compressed(&as_array);
            if v.is_some().into() {
                Ok(v.unwrap())
            } else {
                node.err(&format!("atom is not a G2 point {}", hex::encode(blob)))
            }
        } else {
            node.err(&format!("atom is not G2 size, got {}: Length of bytes object not equal to 96", hex::encode(blob)))
        }
    } else {
         node.err("G2 point is not an atom")
    }
}

fn gt_atom(node: &Node) -> Result<Gt, EvalErr> {
    let atom = node.atom();
    if atom.is_some().into() {
        let blob = atom.unwrap();
        if blob.len() == 288 {
            let mut as_array: [u8; 288] = [0; 288];
            as_array.clone_from_slice(&blob[0..288]);
            let v = Gt::from_compressed(&as_array);
            if v.is_some().into() {
                Ok(v.unwrap())
            } else {
                node.err(&format!("atom is not a Gt point {}", hex::encode(blob)))
            }
        } else {
            node.err(&format!("atom is not Gt size, got {}: Length of bytes object not equal to 288", hex::encode(blob)))
        }
    } else {
         node.err("Gt point is not an atom")
    }
}

pub fn op_bls_g1_subtract(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = BLS_G1_SUBTRACT_BASE_COST;
    check_cost(a, cost, max_cost)?;
    let mut total: G1Projective = G1Projective::identity();
    let mut is_first = true;
    for arg in &args {
        let point = g1_atom(&arg)?;
        cost += BLS_G1_SUBTRACT_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        if is_first {
            total += &point;
        } else {
            total -= &point;
        };
        is_first = false;
    }
    let total: G1Affine = total.into();
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_g1_multiply(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "bls_g1_multiply")?;
    let mut cost = 0;
    let mut total: G1Projective = G1Projective::identity();
    let mut first_iter: bool = true;
    for arg in &args {
        if first_iter {
            let point = g1_atom(&arg)?;
            cost = BLS_G1_MULTIPLY_BASE_COST;
            check_cost(a, cost, max_cost)?;
            total = G1Projective::from(point);
            first_iter = false;
            continue;
        } else {
            let v0 = int_atom(&arg, "bls_g1_multiply")?;
            total *= number_to_scalar(mod_group_order(number_from_u8(v0)));
            cost += v0.len() as Cost * BLS_G1_MULTIPLY_COST_PER_BYTE;
            check_cost(a, cost, max_cost)?;
        }
    }
    let total: G1Affine = total.into();
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_g1_negate(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut args = Node::new(a, input);
    check_arg_count(&args, 1, "bls_g1_negate")?;
    let cost = BLS_G1_NEGATE_BASE_COST;
    check_cost(a, cost, max_cost)?;
    let arg = args.next().unwrap();
    let point = g1_atom(&arg)?;
    let total = -point;
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_g2_add(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = BLS_G2_ADD_BASE_COST;
    let mut total: G2Projective = G2Projective::identity();
    for arg in &args {
        let point = g2_atom(&arg)?;
        cost += BLS_G2_ADD_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        total += &point;
    }
    let total: G2Affine = total.into();
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_g2_subtract(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = BLS_G2_SUBTRACT_BASE_COST;
    let mut total: G2Projective = G2Projective::identity();
    let mut is_first = true;
    for arg in &args {
        let point = g2_atom(&arg)?;
        cost += BLS_G2_SUBTRACT_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        if is_first {
            total += &point;
        } else {
            total -= &point;
        };
        is_first = false;
    }
    let total: G2Affine = total.into();
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_g2_multiply(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "op_bls_g2_multiply")?;
    let mut cost = BLS_G2_MULTIPLY_BASE_COST;
    let mut total: G2Projective = G2Projective::identity();
    let mut first_iter: bool = true;
    for arg in &args {
        if first_iter {
            let point = g2_atom(&arg)?;
            cost += BLS_G2_MULTIPLY_COST_PER_ARG;
            check_cost(a, cost, max_cost)?;
            total = G2Projective::from(point);
            first_iter = false;
            continue;
        } else {
            let v0 = int_atom(&arg, "bls_g2_multiply")?;
            total *= number_to_scalar(mod_group_order(number_from_u8(v0)));
            cost += BLS_G2_MULTIPLY_COST_PER_ARG;
        }
    }
    let total: G2Affine = total.into();
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_g2_negate(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut args = Node::new(a, input);
    check_arg_count(&args, 1, "bls_g2_negate")?;
    let cost = BLS_G2_NEGATE_BASE_COST;
    check_cost(a, cost, max_cost)?;
    let arg = args.next().unwrap();
    let point = g2_atom(&arg)?;
    let total = -point;
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_gt_add(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = BLS_GT_ADD_BASE_COST;
    let mut total: Gt = Gt::identity();
    for arg in &args {
        let point = gt_atom(&arg)?;
        cost += BLS_GT_ADD_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        total += &point;
    }
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_gt_subtract(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = BLS_GT_SUBTRACT_BASE_COST;
    let mut total: Gt = Gt::identity();
    let mut is_first = true;
    for arg in &args {
        let point = gt_atom(&arg)?;
        cost += BLS_GT_SUBTRACT_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        if is_first {
            total += &point;
        } else {
            total -= &point;
        };
        is_first = false;
    }
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_gt_multiply(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "op_bls_gt_multiply")?;
    let mut cost = BLS_GT_MULTIPLY_BASE_COST;
    let mut total: Gt = Gt::identity();
    let mut first_iter: bool = true;
    for arg in &args {
        if first_iter {
            let point = gt_atom(&arg)?;
            cost += BLS_GT_MULTIPLY_COST_PER_ARG;
            check_cost(a, cost, max_cost)?;
            total = Gt::from(point);
            first_iter = false;
            continue;
        } else {
            let v0 = int_atom(&arg, "bls_gt_multiply")?;
            total *= number_to_scalar(mod_group_order(number_from_u8(v0)));
            cost += BLS_GT_MULTIPLY_COST_PER_ARG;
        }
    }
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_gt_negate(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut args = Node::new(a, input);
    check_arg_count(&args, 1, "bls_gt_negate")?;
    let cost = BLS_G2_NEGATE_BASE_COST;
    check_cost(a, cost, max_cost)?;
    let arg = args.next().unwrap();
    let point = gt_atom(&arg)?;
    let total = -point;
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_pairing(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut args = Node::new(a, input);
    let mut cost = BLS_PAIRING_BASE_COST;
    let mut items = Vec::<(G1Affine, G2Prepared)>::new();
    let ac = arg_count(&args, 2);
    if !(1..=2).contains(&ac) {
        return args.err("bls_pairing takes exactly 1 or 2 arguments");
    }

    fn extract_point(g1_node: &Node, g2_node: &Node) -> Result<(G1Affine, G2Prepared), EvalErr> {
        let g2_pair = g2_node.pair();
        if g2_pair.is_some().into() {
            let (g2_pair, right) = g2_pair.unwrap();
            if !right.nullp() {
                return right.err("too many arguments for pairing");
            }
            let p = g1_atom(&g1_node)?;
            let q = g2_atom(&g2_pair)?;
            Ok((p, G2Prepared::from(q)))
        } else {
            g2_node.err("expected atom for G2 point")
        }
    }

    fn extract_points(node: &Node, max_cost: u64) -> Result<(Vec::<(G1Affine, G2Prepared)>, u64), EvalErr>{
        let mut cost = 0;
        let mut items = Vec::<(G1Affine, G2Prepared)>::new();
        let pair = node.pair();
        if pair.is_some().into() {
            let (left, right) = pair.unwrap();

            if !left.nullp() {
                let pair = left.pair();
                if pair.is_some().into() {
                    cost += BLS_PAIRING_COST_PER_ARG;
                    check_cost(&Allocator::new(), cost, max_cost)?;
                    let (left, right) = pair.unwrap();
                    let point = extract_point(&left, &right)?;
                    items.push(point);
                } else {
                    return left.err("expected pair");
                }
            } else {
                return left.err("unexpected null");
            }

            if !right.nullp() {
                let (points, additional_cost) = extract_points(&right, max_cost - cost)?;
                items.extend(points);
                cost += additional_cost;
                check_cost(&Allocator::new(), cost, max_cost)?;
            }
        } else {
            return node.err("expected pair");
        }

        Ok((items, cost))
    }

    let arg = args.next().unwrap();
    match arg.sexp() {
        SExp::Pair(_, __) => {
            let (points, additional_cost) = extract_points(&arg, max_cost - cost)?;
            items.extend(points);
            cost += additional_cost;
            check_cost(&Allocator::new(), cost, max_cost)?;
        },
        SExp::Atom(_) => {
            if ac != 2 {
                let msg = format!("bls_pairing expected second argument on single pairing");
                return args.err(&msg);
            }
            cost += BLS_PAIRING_COST_PER_ARG;
            check_cost(a, cost, max_cost)?;
            let p = g1_atom(&arg)?;
            let arg = args.next().unwrap();
            let q = g2_atom(&arg)?;
            items.push((p, G2Prepared::from(q)));
        }
    }

    let mut item_refs = Vec::<(&G1Affine, &G2Prepared)>::new();
    for (p, q) in items.iter() {
        item_refs.push((&p, &q));
    }
    let total = multi_miller_loop(&item_refs).final_exponentiation();
    new_atom_and_cost(a, cost, &total.to_compressed())
}

pub fn op_bls_map_to_g1(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut args = Node::new(a, input);
    let ac = arg_count(&args, 2);
    if !(1..=2).contains(&ac) {
        return args.err("bls_map_to_g1 takes exactly 1 or 2 arguments");
    }
    let mut cost: Cost = BLS_MAP_TO_G1_BASE_COST;
    let mut dst: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";

    let dst_node: Node;
    let msg_node = args.next().unwrap();
    let msg = atom(&msg_node, "bls_map_to_g1")?;
    cost += msg.len() as Cost * BLS_MAP_TO_G1_COST_PER_BYTE;
    check_cost(a, cost, max_cost)?;

    if ac == 2 {
        dst_node = args.next().unwrap();
        dst = atom(&dst_node, "bls_map_to_g1")?;
        cost += dst.len() as Cost * BLS_MAP_TO_G1_COST_PER_BYTE;
        check_cost(a, cost, max_cost)?;
    }

    let point = <G1Projective as HashToCurve<ExpandMsgXmd<sha2::Sha256>>>::hash_to_curve(msg, dst);
    new_atom_and_cost(a, cost, &G1Affine::from(point).to_compressed())
}

pub fn op_bls_map_to_g2(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut args = Node::new(a, input);
    let ac = arg_count(&args, 2);
    if !(1..=2).contains(&ac) {
        return args.err("bls_map_to_g2 takes exactly 1 or 2 arguments");
    }
    let mut cost: Cost = BLS_MAP_TO_G2_BASE_COST;
    let mut dst: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_";

    let dst_node: Node;
    let msg_node = args.next().unwrap();
    let msg = atom(&msg_node, "bls_map_to_g2")?;
    cost += msg.len() as Cost * BLS_MAP_TO_G2_COST_PER_BYTE;
    check_cost(a, cost, max_cost)?;

    if ac == 2 {
        dst_node = args.next().unwrap();
        dst = atom(&dst_node, "bls_map_to_g2")?;
        cost += dst.len() as Cost * BLS_MAP_TO_G2_COST_PER_BYTE;
        check_cost(a, cost, max_cost)?;
    }

    let point = <G2Projective as HashToCurve<ExpandMsgXmd<sha2::Sha256>>>::hash_to_curve(msg, dst);
    new_atom_and_cost(a, cost, &G2Affine::from(point).to_compressed())
}
