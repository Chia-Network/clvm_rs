use crate::allocator::{Allocator, NodePtr};
use crate::cost::{check_cost, Cost};

use crate::error::{Secp256k1verifyError, Secp256r1verifyError};
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
    check_cost(cost, max_cost)?;

    let [pubkey, msg, sig] = get_args::<3>(a, input, "secp256r1_verify")?;

    // first argument is sec1 encoded pubkey
    let pubkey = atom(a, pubkey, "secp256r1_verify pubkey")?;
    let verifier = P1VerifyingKey::from_sec1_bytes(pubkey.as_ref())
        .map_err(|_| Secp256r1verifyError::PubkeyNotValid(input))?;

    // second arg is sha256 hash of message
    let msg = atom(a, msg, "secp256r1_verify msg")?;
    if msg.as_ref().len() != 32 {
        Err(Secp256r1verifyError::MessageDigestNot32Bytes(input))?;
    }

    // third arg is a fixed-size signature
    let sig = atom(a, sig, "secp256r1_verify sig")?;
    let sig = P1Signature::from_slice(sig.as_ref())
        .map_err(|_| Secp256r1verifyError::SignatureNotValid(input))?;

    // verify signature
    let result = verifier.verify_prehash(msg.as_ref(), &sig);

    if result.is_err() {
        Err(Secp256r1verifyError::Failed(input))?
    } else {
        Ok(Reduction(cost, a.nil()))
    }
}

// expects: pubkey msg sig
pub fn op_secp256k1_verify(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let cost = SECP256K1_VERIFY_COST;
    check_cost(cost, max_cost)?;

    let [pubkey, msg, sig] = get_args::<3>(a, input, "secp256k1_verify")?;

    // first argument is sec1 encoded pubkey
    let pubkey = atom(a, pubkey, "secp256k1_verify pubkey")?;
    let verifier = K1VerifyingKey::from_sec1_bytes(pubkey.as_ref())
        .map_err(|_| Secp256k1verifyError::PubkeyNotValid(input))?;

    // second arg is message
    let msg = atom(a, msg, "secp256k1_verify msg")?;
    if msg.as_ref().len() != 32 {
        Err(Secp256k1verifyError::MessageDigestNot32Bytes(input))?;
    }

    // third arg is a fixed-size signature
    let sig = atom(a, sig, "secp256k1_verify sig")?;
    let sig = K1Signature::from_slice(sig.as_ref())
        .map_err(|_| Secp256k1verifyError::SignatureNotValid(input))?;

    // verify signature
    let result = verifier.verify_prehash(msg.as_ref(), &sig);

    if result.is_err() {
        Err(Secp256k1verifyError::Failed(input))?
    } else {
        Ok(Reduction(cost, a.nil()))
    }
}
