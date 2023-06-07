use crate::allocator::{Allocator, NodePtr};
use crate::cost::{check_cost, Cost};
use crate::err_utils::err;
use crate::op_utils::{atom, get_args};
use crate::reduction::{Reduction, Response};
use k256::ecdsa::{Signature as K1Signature, VerifyingKey as K1VerifyingKey};
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use p256::ecdsa::{Signature as P1Signature, VerifyingKey as P1VerifyingKey};

const SECP256R1_VERIFY_COST: Cost = 1850000;
const SECP256K1_VERIFY_COST: Cost = 1300000;

// expects: pubkey msg sig
pub fn op_secp256r1_verify(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let cost = SECP256R1_VERIFY_COST;
    check_cost(a, cost, max_cost)?;

    let [pubkey, msg, sig] = get_args::<3>(a, input, "secp256r1_verify")?;

    // first argument is sec1 encoded pubkey
    let pubkey = atom(a, pubkey, "secp256r1_verify pubkey")?;
    let verifier = P1VerifyingKey::from_sec1_bytes(pubkey)
        .or_else(|_| err(input, "secp256r1_verify pubkey is not valid"))?;

    // second arg is sha256 hash of message
    let msg = atom(a, msg, "secp256r1_verify msg")?;
    if msg.len() != 32 {
        return err(input, "secp256r1_verify message digest is not 32 bytes");
    }

    // third arg is a fixed-size signature
    let sig = atom(a, sig, "secp256r1_verify sig")?;
    let sig = P1Signature::from_slice(sig)
        .or_else(|_| err(input, "secp256r1_verify sig is not valid"))?;

    // verify signature
    let result = verifier.verify_prehash(msg, &sig);

    if result.is_err() {
        err(input, "secp256r1_verify failed")
    } else {
        Ok(Reduction(cost, a.null()))
    }
}

// expects: pubkey msg sig
pub fn op_secp256k1_verify(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let cost = SECP256K1_VERIFY_COST;
    check_cost(a, cost, max_cost)?;

    let [pubkey, msg, sig] = get_args::<3>(a, input, "secp256k1_verify")?;

    // first argument is sec1 encoded pubkey
    let pubkey = atom(a, pubkey, "secp256k1_verify pubkey")?;
    let verifier = K1VerifyingKey::from_sec1_bytes(pubkey)
        .or_else(|_| err(input, "secp256k1_verify pubkey is not valid"))?;

    // second arg is message
    let msg = atom(a, msg, "secp256k1_verify msg")?;
    if msg.len() != 32 {
        return err(input, "secp256k1_verify message digest is not 32 bytes");
    }

    // third arg is a fixed-size signature
    let sig = atom(a, sig, "secp256k1_verify sig")?;
    let sig = K1Signature::from_slice(sig)
        .or_else(|_| err(input, "secp256k1_verify sig is not valid"))?;

    // verify signature
    let result = verifier.verify_prehash(msg, &sig);

    if result.is_err() {
        err(input, "secp256k1_verify failed")
    } else {
        Ok(Reduction(cost, a.null()))
    }
}
