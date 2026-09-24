use anchor_lang::prelude::*;

/// `FeedStatus` as stored on `PriceFeed.status`. Unrecognized bytes are Uninit.
pub const FEED_STATUS_UNINIT: u8 = 0;
pub const FEED_STATUS_LIVE: u8 = 1;
pub const FEED_STATUS_FROZEN: u8 = 2;

/// Anchor `account:PriceFeed` discriminator (`sha256("account:PriceFeed")[0..8]`).
pub const PRICE_FEED_DISCRIMINATOR: [u8; 8] = [189, 103, 252, 23, 152, 35, 243, 156];

/// Zero-copy `PriceFeed` payload after the 8-byte discriminator. Layout is pinned to the Canary
/// program's `PriceFeed` state (192 bytes, 8-aligned).
#[account(zero_copy)]
#[repr(C)]
pub struct PriceFeed {
    pub value: [u8; 16],
    pub unix_timestamp: u64,
    pub signer_group: Pubkey,
    pub entry_id: u16,
    pub exp: u8,
    pub status: u8,
    pub bump: u8,
    pub _reserved: [u8; 131],
}

impl PriceFeed {
    const DATA_LEN: usize = 192;
    pub const ACCOUNT_LEN: usize = 8 + Self::DATA_LEN;

    pub fn value_u128(&self) -> u128 {
        u128::from_le_bytes(self.value)
    }
}

const _: () = assert!(std::mem::size_of::<PriceFeed>() == PriceFeed::DATA_LEN);
const _: () = assert!(std::mem::align_of::<PriceFeed>() == 8);
