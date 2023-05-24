use crate::allocator::{Allocator, NodePtr};
use crate::cost::{check_cost, Cost};
use crate::node::Node;
use crate::op_utils::{atom, check_arg_count};
use crate::reduction::{Reduction, Response};
use k256::ecdsa::{VerifyingKey as K1VerifyingKey, Signature as K1Signature};
use p256::ecdsa::{VerifyingKey as P1VerifyingKey, Signature as P1Signature};
use p256::ecdsa::signature::hazmat::PrehashVerifier;

const SECP256P1_VERIFY_COST: Cost = 3000000;
const SECP256K1_VERIFY_COST: Cost = 3000000;

// expects: pubkey msg sig
pub fn op_secp256p1_verify(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 3, "secp256p1_verify")?;

    let cost = SECP256P1_VERIFY_COST;
    check_cost(a, cost, max_cost)?;

    // first argument is sec1 encoded pubkey
    let pubkey = atom(args.first()?, "secp256p1_verify pubkey")?;
    let Ok(verifier) = P1VerifyingKey::from_sec1_bytes(pubkey) else { return args.err("secp256p1_verify pubkey is not valid") };

    // second arg is message
    let args = args.rest()?;
    let msg = atom(args.first()?, "secp256p1_verify msg")?;

    // third arg is der encoded sig
    let args = args.rest()?;
    let sig = atom(args.first()?, "secp256p1_verify sig")?;
    let Ok(sig) = P1Signature::from_slice(sig) else { return args.err("secp256p1_verify sig is not valid") };

    // verify signature
    let result = verifier.verify_prehash(msg, &sig);

    if !result.is_ok() {
        Node::new(a, input).err("secp256p1_verify failed")
    } else {
        Ok(Reduction(cost, a.null()))
    }
}

// expects: pubkey msg sig
pub fn op_secp256k1_verify(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 3, "secp256k1_verify")?;

    let cost = SECP256K1_VERIFY_COST;
    check_cost(a, cost, max_cost)?;

    // first argument is sec1 encoded pubkey
    let pubkey = atom(args.first()?, "secp256k1_verify pubkey")?;
    let Ok(verifier) = K1VerifyingKey::from_sec1_bytes(pubkey) else { return args.err("secp256k1_verify pubkey is not valid") };

    // second arg is message
    let args = args.rest()?;
    let msg = atom(args.first()?, "secp256k1_verify msg")?;

    // third arg is der encoded sig
    let args = args.rest()?;
    let sig = atom(args.first()?, "secp256k1_verify sig")?;
    let Ok(sig) = K1Signature::from_slice(sig) else { return args.err("secp256k1_verify sig is not valid") };

    // verify signature
    let result = verifier.verify_prehash(msg, &sig);

    if !result.is_ok() {
        Node::new(a, input).err("secp256k1_verify failed")
    } else {
        Ok(Reduction(cost, a.null()))
    }
}

