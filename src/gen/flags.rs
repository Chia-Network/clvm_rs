// flags controlling to condition parsing

// in conditions output, integers must use canonical encoding (i.e. no redundant
// leading zeros)
pub const COND_CANON_INTS: u32 = 0x010000;

// unknown condition codes are disallowed
pub const NO_UNKNOWN_CONDS: u32 = 0x20000;
