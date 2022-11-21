use crate::sha2::{Digest, Sha256};

pub type Bytes32 = [u8; 32];

pub fn hash_blob(blob: &[u8]) -> Bytes32 {
    let mut sha256 = Sha256::new();
    sha256.update(blob);
    sha256.finalize().into()
}

pub fn hash_blobs(blobs: &[&[u8]]) -> Bytes32 {
    let mut sha256 = Sha256::new();
    for blob in blobs.iter() {
        sha256.update(blob);
    }
    sha256.finalize().into()
}
