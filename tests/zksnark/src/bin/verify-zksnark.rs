use bls12_381::{multi_miller_loop, G1Affine, G1Projective, G2Affine, G2Prepared, Scalar};
use clvmr::allocator::{Allocator, AtomBuf, Checkpoint, NodePtr, SExp};
use clvmr::chia_dialect::ENABLE_BLS_OPS_OUTSIDE_GUARD;
use clvmr::dialect::{Dialect, OperatorSet};
use clvmr::reduction::Reduction;
use clvmr::run_program::run_program;
use clvmr::serde::node_from_bytes;
use clvmr::ChiaDialect;
use group::Group;
use hex::FromHex;
use num_bigint::{BigInt, Sign};
use serde::Deserialize;
use serde_json::Result;
use std::fs::File;
use std::io::Read;

#[derive(Debug, Deserialize)]
struct VerificationKey {
    vk_alpha_1: Vec<String>,
    vk_beta_2: Vec<Vec<String>>,
    vk_gamma_2: Vec<Vec<String>>,
    vk_delta_2: Vec<Vec<String>>,
    vk_alphabeta_12: Vec<Vec<Vec<String>>>,
    #[serde(alias = "IC")]
    ic: Vec<Vec<String>>,
    #[serde(alias = "nPublic")]
    n_public: u8,
    protocol: String,
    curve: String,
}

#[derive(Debug, Deserialize)]
struct Proof {
    pi_a: Vec<String>,
    pi_b: Vec<Vec<String>>,
    pi_c: Vec<String>,
    protocol: String,
    curve: String,
}

fn bigint_to_48bytes(i: &BigInt) -> [u8; 48] {
    fn prepend<T: Clone>(v: &mut Vec<T>, x: T, n: usize) {
        v.resize(v.len() + n, x);
        v.rotate_right(n);
    }

    let mut out: Vec<u8> = i.to_bytes_be().1;
    let len = out.len();
    prepend(&mut out, 0, 48 - len);
    out.try_into().unwrap()
}

fn vec_pair(arr: &Vec<String>) -> ([u8; 48], [u8; 48]) {
    (
        bigint_to_48bytes(&arr[0].clone().parse::<BigInt>().unwrap()),
        bigint_to_48bytes(&arr[1].clone().parse::<BigInt>().unwrap()),
    )
}

fn vec_pair_g1(arr: &Vec<String>) -> G1Affine {
    let (fp_1, fp_2) = vec_pair(&arr);
    let data: [u8; 96] = [fp_1, fp_2].concat().try_into().unwrap();
    G1Affine::from_uncompressed(&data).unwrap()
}

fn vec_pair_g2(arr: &Vec<Vec<String>>) -> G2Affine {
    let (fp_1, fp_2) = vec_pair(&arr[0]);
    let (fp_3, fp_4) = vec_pair(&arr[1]);
    let data: [u8; 192] = [fp_2, fp_1, fp_4, fp_3].concat().try_into().unwrap();
    let p = G2Affine::from_uncompressed(&data);
    p.unwrap()
}

pub fn main() {
    println!("verifying zksnark");

    // Read verification_key.json
    let mut file = File::open("data/verification_key.json").unwrap();
    let mut verification_key = String::new();
    file.read_to_string(&mut verification_key).unwrap();
    let verification_key: VerificationKey = serde_json::from_str(&verification_key)
        .expect("Verification Key JSON was not well-formatted");

    // Read public.json
    let mut file = File::open("data/public.json").unwrap();
    let mut public = String::new();
    file.read_to_string(&mut public).unwrap();
    let public: Vec<String> =
        serde_json::from_str(&public).expect("Public JSON was not well-formatted");

    // Read proof.json
    let mut file = File::open("data/proof.json").unwrap();
    let mut proof = String::new();
    file.read_to_string(&mut proof).unwrap();
    let proof: Proof = serde_json::from_str(&proof).expect("Proof JSON was not well-formatted");

    let ic0 = vec_pair_g1(&verification_key.ic[0]);

    let mut cpub = G1Affine::identity();
    for (i, public_i) in public.iter().enumerate() {
        let ic = vec_pair_g1(&verification_key.ic[i + 1]);
        let scalar: [u8; 32] = public_i
            .parse::<BigInt>()
            .unwrap()
            .to_bytes_le()
            .1
            .try_into()
            .unwrap();
        let scalar = Scalar::from_bytes(&scalar).unwrap();
        cpub = (cpub + ic * scalar).into();
    }
    cpub = (cpub + G1Projective::from(ic0)).into();

    let pi_a = vec_pair_g1(&proof.pi_a);
    let pi_b = vec_pair_g2(&proof.pi_b);
    let pi_c = vec_pair_g1(&proof.pi_c);

    let vk_gamma_2 = vec_pair_g2(&verification_key.vk_gamma_2);
    let vk_delta_2 = vec_pair_g2(&verification_key.vk_delta_2);
    let vk_alpha_1 = vec_pair_g1(&verification_key.vk_alpha_1);
    let vk_beta_2 = vec_pair_g2(&verification_key.vk_beta_2);

    // output the compressed values
    println!(
        "bls_pairing_identity 0x{} 0x{} 0x{} 0x{} 0x{} 0x{} 0x{} 0x{} => 0 | 7800000",
        hex::encode((-pi_a).to_compressed()),
        hex::encode(pi_b.to_compressed()),
        hex::encode(cpub.to_compressed()),
        hex::encode(vk_gamma_2.to_compressed()),
        hex::encode(pi_c.to_compressed()),
        hex::encode(vk_delta_2.to_compressed()),
        hex::encode(vk_alpha_1.to_compressed()),
        hex::encode(vk_beta_2.to_compressed())
    );

    // run the miller loop
    let mut item_refs = Vec::<(&G1Affine, &G2Prepared)>::new();
    let pi_a = -pi_a;
    let pi_b = G2Prepared::from(pi_b);
    let vk_gamma_2 = G2Prepared::from(vk_gamma_2);
    let vk_delta_2 = G2Prepared::from(vk_delta_2);
    let vk_beta_2 = G2Prepared::from(vk_beta_2);
    item_refs.push((&pi_a, &pi_b));
    item_refs.push((&cpub, &vk_gamma_2));
    item_refs.push((&pi_c, &vk_delta_2));
    item_refs.push((&vk_alpha_1, &vk_beta_2));
    let identity: bool = multi_miller_loop(&item_refs)
        .final_exponentiation()
        .is_identity()
        .into();
    println!("identity: {:?}", identity);
}
