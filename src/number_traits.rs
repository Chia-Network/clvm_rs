
pub trait NumberTraits {
    fn from_unsigned_bytes_be(v: &[u8]) -> Self;
    fn zero() -> Self;
    fn from_u8(v: &[u8]) -> Self;
    fn to_u64(n: &Self) -> u64;
    fn div_mod_floor(&self, denominator: &Self) -> (Self, Self) where Self: Sized;
    fn mod_floor(&self, denominator: &Self) -> Self;
}

#[cfg(test)]
pub trait TestNumberTraits {
    fn from_str_radix(s: &str, radix: i32) -> Self;
}
