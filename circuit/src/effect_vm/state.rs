/// State column offsets (relative to state start).
pub const BALANCE_LO: usize = 0;
pub const BALANCE_HI: usize = 1;
pub const NONCE: usize = 2;
pub const FIELD_BASE: usize = 3; // fields[0..8] at offsets 3..11
pub const CAP_ROOT: usize = 11;
pub const STATE_COMMIT: usize = 12;
pub const RESERVED: usize = 13;
pub const SIZE: usize = 14;
