#[cfg(feature = "openssl")]
use openssl;

#[cfg(not(feature = "openssl"))]
use sha2::Digest;

#[derive(Default, Clone)]
pub struct Sha256 {
    #[cfg(feature = "openssl")]
    ctx: openssl::sha::Sha256,

    #[cfg(not(feature = "openssl"))]
    ctx: sha2::Sha256,
}

impl Sha256 {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, buf: impl AsRef<[u8]>) {
        self.ctx.update(buf.as_ref());
    }
    pub fn finalize(self) -> [u8; 32] {
        #[cfg(feature = "openssl")]
        {
            self.ctx.finish()
        }
        #[cfg(not(feature = "openssl"))]
        {
            self.ctx.finalize().into()
        }
    }
}

#[test]
fn test_sha256() {
    // https://www.di-mgt.com.au/sha_testvectors.html

    let output = &[
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];

    let mut ctx = Sha256::new();
    ctx.update([0x61, 0x62, 0x63]);
    assert_eq!(&ctx.finalize().as_slice(), output);

    let mut ctx = Sha256::new();
    ctx.update([0x61]);
    ctx.update([0x62]);
    ctx.update([0x63]);
    assert_eq!(&ctx.finalize().as_slice(), output);

    let mut ctx = Sha256::new();
    ctx.update([0x61, 0x62]);
    ctx.update([0x63]);
    assert_eq!(&ctx.finalize().as_slice(), output);

    let mut ctx = Sha256::new();
    ctx.update([0x61]);
    ctx.update([0x62, 0x63]);
    assert_eq!(&ctx.finalize().as_slice(), output);
}
