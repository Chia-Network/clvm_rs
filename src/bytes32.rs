use crate::sha2::{Digest, Sha256};
use hex::encode;

use std::fmt::{Debug, Formatter};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Bytes32(pub [u8; 32]);

impl AsRef<[u8]> for Bytes32 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Debug for Bytes32 {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(&encode(self.0))?;
        Ok(())
    }
}

pub fn hash_blob(blob: &[u8]) -> Bytes32 {
    let mut sha256 = Sha256::new();
    sha256.update(blob);
    Bytes32(sha256.finalize().into())
}

pub fn hash_blobs(blobs: &[&[u8]]) -> Bytes32 {
    let mut sha256 = Sha256::new();
    for blob in blobs.iter() {
        sha256.update(blob);
    }
    Bytes32(sha256.finalize().into())
}
