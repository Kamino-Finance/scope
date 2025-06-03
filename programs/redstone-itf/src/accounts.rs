use anchor_lang::prelude::*;

pub const RESERVED_BYTE_SIZE: usize = 64;
pub const U256_BYTE_SIZE: usize = 256 / 8;
pub const U64_START_INDEX: usize = U256_BYTE_SIZE - 8;

#[account]
pub struct PriceData {
    pub feed_id: [u8; U256_BYTE_SIZE],
    pub value: [u8; U256_BYTE_SIZE],
    // `timestamp` is when the price was computed...
    pub timestamp: u64,
    // ... while `write_timestamp` is when the price was pushed to the account
    pub write_timestamp: Option<u64>,
    pub write_slot_number: u64,
    pub decimals: u8,
    pub reserved: [u8; RESERVED_BYTE_SIZE],
}
