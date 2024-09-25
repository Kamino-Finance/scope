use std::convert::TryInto;

use anchor_lang::prelude::*;

use self::switchboard::*;
use super::switchboard_v2::validate_confidence;
use crate::{DatedPrice, Price, ScopeError};

const MAX_EXPONENT: u32 = 15;

pub fn get_price(
    switchboard_feed_info: &AccountInfo,
) -> std::result::Result<DatedPrice, ScopeError> {
    let feed_buffer = switchboard_feed_info
        .try_borrow_data()
        .map_err(|_| ScopeError::SwitchboardOnDemandError)?;
    let feed = PullFeedAccountData::parse(feed_buffer)?;

    let price_switchboard_desc = feed
        .result
        .value()
        .ok_or(ScopeError::SwitchboardOnDemandError)?;
    let price: Price = price_switchboard_desc.try_into()?;

    if !cfg!(feature = "skip_price_validation") {
        let std_dev = feed
            .result
            .std_dev()
            .ok_or(ScopeError::SwitchboardOnDemandError)?;
        if validate_confidence(
            price_switchboard_desc.mantissa(),
            price_switchboard_desc.scale(),
            std_dev.mantissa(),
            std_dev.scale(),
        )
        .is_err()
        {
            msg!(
                    "Validation of confidence interval for SB On-Demand feed {} failed. Price: {:?}, stdev_mantissa: {:?}, stdev_scale: {:?}",
                    switchboard_feed_info.key(),
                    price,
                    std_dev.mantissa(),
                    std_dev.scale()
                );
            return Err(ScopeError::SwitchboardOnDemandError);
        }
    };

    // NOTE: This is the slot and timestamp of the selected sample,
    // not necessarily the most recent one.
    let last_updated_slot = feed.result.slot;
    let unix_timestamp = 0;

    Ok(DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        ..Default::default()
    })
}

pub fn validate_price_account(switchboard_feed_info: &Option<AccountInfo>) -> crate::Result<()> {
    if cfg!(feature = "skip_price_validation") {
        return Ok(());
    }
    let Some(switchboard_feed_info) = switchboard_feed_info else {
        msg!("No pyth pull price account provided");
        return err!(ScopeError::PriceNotValid);
    };
    let feed_buffer = switchboard_feed_info
        .try_borrow_data()
        .map_err(|_| ScopeError::SwitchboardOnDemandError)?;
    let _feed = PullFeedAccountData::parse(feed_buffer)?;
    Ok(())
}

impl TryFrom<rust_decimal::Decimal> for Price {
    type Error = ScopeError;

    fn try_from(sb_decimal: rust_decimal::Decimal) -> std::result::Result<Self, Self::Error> {
        if sb_decimal.mantissa() < 0 {
            msg!("Switchboard v2 oracle price feed is negative");
            return Err(ScopeError::PriceNotValid);
        }
        let (exp, value) = if sb_decimal.scale() > MAX_EXPONENT {
            // exp is capped. Remove the extra digits from the mantissa.
            let exp_diff = sb_decimal
                .scale()
                .checked_sub(MAX_EXPONENT)
                .ok_or(ScopeError::MathOverflow)?;
            let factor = 10_i128
                .checked_pow(exp_diff)
                .ok_or(ScopeError::MathOverflow)?;
            // Loss of precision here is expected.
            let value = sb_decimal.mantissa() / factor;
            (MAX_EXPONENT, value)
        } else {
            (sb_decimal.scale(), sb_decimal.mantissa())
        };
        let exp: u64 = exp.into();
        let value: u64 = value.try_into().map_err(|_| ScopeError::IntegerOverflow)?;
        Ok(Price { value, exp })
    }
}

pub mod switchboard {
    use std::cell::Ref;

    use rust_decimal::Decimal;
    use solana_program::pubkey::Pubkey;

    use crate::ScopeError;

    pub const PRECISION: u32 = 18;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
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

    #[repr(C)]
    #[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct OracleSubmission {
        /// The public key of the oracle that submitted this value.
        pub oracle: Pubkey,
        /// The slot at which this value was signed.
        pub slot: u64,
        padding1: [u8; 8],
        /// The value that was submitted.
        pub value: i128,
    }

    /// A representation of the data in a pull feed account.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
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

    impl PullFeedAccountData {
        pub fn parse<'info>(data: Ref<'info, &mut [u8]>) -> Result<Ref<'info, Self>, ScopeError> {
            if data.len() < Self::discriminator().len() {
                return Err(ScopeError::InvalidAccountDiscriminator);
            }

            let mut disc_bytes = [0u8; 8];
            disc_bytes.copy_from_slice(&data[..8]);
            if disc_bytes != Self::discriminator() {
                return Err(ScopeError::InvalidAccountDiscriminator);
            }

            Ok(Ref::map(data, |data: &&mut [u8]| {
                bytemuck::from_bytes(&data[8..std::mem::size_of::<Self>() + 8])
            }))
        }

        pub fn discriminator() -> [u8; 8] {
            [196, 27, 108, 196, 10, 215, 219, 40]
        }

        pub fn parse_data(data: &[u8]) -> Result<&Self, ScopeError> {
            if data.len() < Self::discriminator().len() {
                return Err(ScopeError::InvalidAccountDiscriminator);
            }

            let mut disc_bytes = [0u8; 8];
            disc_bytes.copy_from_slice(&data[..8]);
            if disc_bytes != Self::discriminator() {
                return Err(ScopeError::InvalidAccountDiscriminator);
            }

            Ok(bytemuck::from_bytes(
                &data[8..std::mem::size_of::<Self>() + 8],
            ))
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct CompactResult {
        /// The standard deviation of the submissions needed for quorom size
        pub std_dev: f32,
        /// The mean of the submissions needed for quorom size
        pub mean: f32,
        /// The slot at which this value was signed.
        pub slot: u64,
    }
}
