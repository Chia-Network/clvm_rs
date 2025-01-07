use rand::Rng;
use std::hash::{BuildHasher, Hasher};

#[derive(Default, Clone, Copy)]
pub struct IdentityHash(u64, u64);

impl IdentityHash {
    fn new(salt: u64) -> Self {
        Self(0, salt)
    }
}

impl Hasher for IdentityHash {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0 =
            u64::from_le_bytes(bytes[0..8].try_into().expect("expected 32 byte hashes")) ^ self.1;
    }

    fn write_u64(&mut self, _i: u64) {
        panic!("This hasher only takes bytes");
    }
}

#[derive(Clone)]
pub struct RandomState(u64);

impl Default for RandomState {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        Self(rng.gen())
    }
}

impl BuildHasher for RandomState {
    type Hasher = IdentityHash;

    fn build_hasher(&self) -> Self::Hasher {
        IdentityHash::new(self.0)
    }
}
