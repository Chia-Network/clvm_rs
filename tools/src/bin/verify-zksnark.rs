use chia_bls::{aggregate_pairing, G1Element, G2Element};
use num_bigint::BigInt;
use serde::Deserialize;

use std::fs::File;
use std::io::Read;

#[allow(dead_code)]
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

#[allow(dead_code)]
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

fn vec_pair(arr: &[String]) -> ([u8; 48], [u8; 48]) {
    (
        bigint_to_48bytes(&arr[0].clone().parse::<BigInt>().unwrap()),
        bigint_to_48bytes(&arr[1].clone().parse::<BigInt>().unwrap()),
    )
}

fn vec_pair_g1(arr: &[String]) -> G1Element {
    let (fp_1, fp_2) = vec_pair(arr);
    let data: [u8; 96] = [fp_1, fp_2].concat().try_into().unwrap();
    println!("G1 uncompressed: {}", hex::encode(data));
    let ret = G1Element::from_uncompressed(&data).unwrap();
    println!("G1 compressed: {}", hex::encode(ret.to_bytes()));
    ret
}

fn vec_pair_g2(arr: &[Vec<String>]) -> G2Element {
    let (fp_1, fp_2) = vec_pair(&arr[0]);
    let (fp_3, fp_4) = vec_pair(&arr[1]);
    let data: [u8; 192] = [fp_2, fp_1, fp_4, fp_3].concat().try_into().unwrap();
    println!("G2 uncompressed: {}", hex::encode(data));
    let ret = G2Element::from_uncompressed(&data).unwrap();
    println!("G2 compressed: {}", hex::encode(ret.to_bytes()));
    ret
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

    let mut cpub = G1Element::default();
    for (i, public_i) in public.iter().enumerate() {
        let mut ic = vec_pair_g1(&verification_key.ic[i + 1]);
        let scalar = public_i.parse::<BigInt>().unwrap().to_bytes_be().1;
        ic.scalar_multiply(&scalar);
        cpub += &ic;
    }
    cpub += &ic0;

    let mut pi_a = vec_pair_g1(&proof.pi_a);
    pi_a.negate();
    let pi_b = vec_pair_g2(&proof.pi_b);
    let pi_c = vec_pair_g1(&proof.pi_c);

    let vk_gamma_2 = vec_pair_g2(&verification_key.vk_gamma_2);
    let vk_delta_2 = vec_pair_g2(&verification_key.vk_delta_2);
    let vk_alpha_1 = vec_pair_g1(&verification_key.vk_alpha_1);
    let vk_beta_2 = vec_pair_g2(&verification_key.vk_beta_2);

    // output the compressed values
    println!(
        "bls_pairing_identity 0x{} 0x{} 0x{} 0x{} 0x{} 0x{} 0x{} 0x{} => 0 | 7800000",
        hex::encode(pi_a.to_bytes()),
        hex::encode(pi_b.to_bytes()),
        hex::encode(cpub.to_bytes()),
        hex::encode(vk_gamma_2.to_bytes()),
        hex::encode(pi_c.to_bytes()),
        hex::encode(vk_delta_2.to_bytes()),
        hex::encode(vk_alpha_1.to_bytes()),
        hex::encode(vk_beta_2.to_bytes())
    );

    // run the miller loop
    let item_refs: Vec<(&G1Element, &G2Element)> = vec![
        (&pi_a, &pi_b),
        (&cpub, &vk_gamma_2),
        (&pi_c, &vk_delta_2),
        (&vk_alpha_1, &vk_beta_2),
    ];
    let identity: bool = aggregate_pairing(item_refs);
    assert!(identity);
}
