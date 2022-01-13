pub trait NumberTraits {
    fn from_unsigned_bytes_be(v: &[u8]) -> Self;
    fn to_signed_bytes(&self) -> Vec<u8>;
    fn zero() -> Self;
    fn from_u8(v: &[u8]) -> Self;
    fn to_u64(&self) -> u64;
    fn div_mod_floor(&self, denominator: &Self) -> (Self, Self)
    where
        Self: Sized;
    fn mod_floor(&self, denominator: &Self) -> Self;
    fn equal(&self, other: i64) -> bool;
    fn not_equal(&self, other: i64) -> bool;
    fn greater_than(&self, other: u64) -> bool;
}

#[cfg(test)]
pub trait TestNumberTraits {
    fn from_str_radix(s: &str, radix: i32) -> Self;
}
