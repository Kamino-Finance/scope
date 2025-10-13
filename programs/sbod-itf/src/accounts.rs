use anchor_lang::prelude::*;
use rust_decimal::Decimal;
use solana_program::pubkey::Pubkey;

pub const PRECISION: u32 = 18;

#[derive(Debug)]
#[zero_copy]
pub struct CurrentResult {
    /// The median value of the submissions needed for quorom size
    pub value: i128,
    /// The standard deviation of the submissions needed for quorom size
    pub std_dev: i128,
    /// The mean of the submissions needed for quorom size
    pub mean: i128,
    /// The range of the submissions needed for quorom size
    pub range: i128,
    /// The minimum value of the submissions needed for quorom size
    pub min_value: i128,
    /// The maximum value of the submissions needed for quorom size
    pub max_value: i128,
    /// The number of samples used to calculate this result
    pub num_samples: u8,
    pub padding1: [u8; 7],
    /// The slot at which this value was signed.
    pub slot: u64,
    /// The slot at which the first considered submission was made
    pub min_slot: u64,
    /// The slot at which the last considered submission was made
    pub max_slot: u64,
}
impl CurrentResult {
    /// The median value of the submissions needed for quorom size
    pub fn value(&self) -> Option<Decimal> {
        if self.slot == 0 {
            return None;
        }
        Some(Decimal::from_i128_with_scale(self.value, PRECISION))
    }

    /// The standard deviation of the submissions needed for quorom size
    pub fn std_dev(&self) -> Option<Decimal> {
        if self.slot == 0 {
            return None;
        }
        Some(Decimal::from_i128_with_scale(self.std_dev, PRECISION))
    }
}

#[derive(Debug)]
#[zero_copy]
pub struct OracleSubmission {
    /// The public key of the oracle that submitted this value.
    pub oracle: Pubkey,
    /// The slot at which this value was signed.
    pub slot: u64,
    padding1: [u8; 8],
    /// The value that was submitted.
    pub value: i128,
}

static_assertions::const_assert_eq!(3200, std::mem::size_of::<PullFeedAccountData>());

/// A representation of the data in a pull feed account.
#[derive(Debug)]
#[account(zero_copy)]
pub struct PullFeedAccountData {
    /// The oracle submissions for this feed.
    pub submissions: [OracleSubmission; 32],
    /// The public key of the authority that can update the feed hash that
    /// this account will use for registering updates.
    pub authority: Pubkey,
    /// The public key of the queue which oracles must be bound to in order to
    /// submit data to this feed.
    pub queue: Pubkey,
    /// SHA-256 hash of the job schema oracles will execute to produce data
    /// for this feed.
    pub feed_hash: [u8; 32],
    /// The slot at which this account was initialized.
    pub initialized_at: i64,
    pub permissions: u64,
    pub max_variance: u64,
    pub min_responses: u32,
    pub name: [u8; 32],
    padding1: [u8; 2],
    pub historical_result_idx: u8,
    pub min_sample_size: u8,
    pub last_update_timestamp: i64,
    pub lut_slot: u64,
    _reserved1: [u8; 32],
    pub result: CurrentResult,
    pub max_staleness: u32,
    padding2: [u8; 12],
    pub historical_results: [CompactResult; 32],
    _ebuf4: [u8; 8],
    _ebuf3: [u8; 24],
    _ebuf2: [u8; 256],
}

#[derive(Debug)]
#[zero_copy]
pub struct CompactResult {
    /// The standard deviation of the submissions needed for quorom size
    pub std_dev: f32,
    /// The mean of the submissions needed for quorom size
    pub mean: f32,
    /// The slot at which this value was signed.
    pub slot: u64,
}
