use std::convert::TryInto;

use anchor_lang::prelude::*;

use self::switchboard::*;
use crate::{DatedPrice, Price, Result, ScopeError};

const MAX_EXPONENT: u32 = 10;

const MIN_CONFIDENCE_PERCENTAGE: u64 = 2u64;
const CONFIDENCE_FACTOR: u64 = 100 / MIN_CONFIDENCE_PERCENTAGE;

pub fn get_price(
    switchboard_feed_info: &AccountInfo,
) -> std::result::Result<DatedPrice, ScopeError> {
    let feed_buffer = switchboard_feed_info.borrow()
        .map_err(|_| ScopeError::SwitchboardOnDemandError)?;
    let feed = PullFeedAccountData::parse(&feed_buffer)?;

    let price_switchboard_desc = feed.result()
        .ok_or(ScopeError::SbOnDemandError)?;
    let price: Price = price_switchboard_desc.try_into()?;

    if !cfg!(feature = "skip_price_validation") {
        let std_dev = feed.result.std_dev().ok_or(ScopeError::SbOnDemandError)?;
        if validate_confidence(
            price_switchboard_desc.mantissa(),
            price_switchboard_desc.scale(),
            std_dev.mantissa(),
            std_dev.scale(),
        )
        .is_err()
        {
            // Using sol log because with exactly 5 parameters, msg! expect u64s.
            msg!(
                    "Validation of confidence interval for SB On-Demand feed {} failed. Price: {:?}, stdev_mantissa: {:?}, stdev_scale: {:?}",
                    switchboard_feed_info.key(),
                    price,
                    std_dev.mantissa(),
                    std_dev.scale()
                );
            return Err(ScopeError::SbOnDemandError);
        }
    };

    // NOTE: This is the slot and timestamp of the selected sample,
    // not necessarily the most recent one.
    let last_updated_slot = feed.result.slot;
    let unix_timestamp = feed
        .result
        .timestamp
        .try_into()
        .unwrap();

    Ok(DatedPrice {
        price,
        last_updated_slot,
        unix_timestamp,
        ..Default::default()
    })
}

fn validate_confidence(
    price_mantissa: i128,
    price_scale: u32,
    stdev_mantissa: i128,
    stdev_scale: u32,
) -> std::result::Result<(), ScopeError> {
    // Step 1: compute scaling factor to bring the stdev to the same scale as the price.
    let (scale_op, scale_diff): (&dyn Fn(i128, i128) -> Option<i128>, _) =
        if price_scale >= stdev_scale {
            (
                &i128::checked_mul,
                price_scale.checked_sub(stdev_scale).unwrap(),
            )
        } else {
            (
                &i128::checked_div,
                stdev_scale.checked_sub(price_scale).unwrap(),
            )
        };

    let scaling_factor = 10_i128
        .checked_pow(scale_diff)
        .ok_or(ScopeError::MathOverflow)?;

    // Step 2: multiply the stdev by the CONFIDENCE_FACTOR and apply scaling factor.

    let stdev_x_confidence_factor_scaled = stdev_mantissa
        .checked_mul(CONFIDENCE_FACTOR.into())
        .and_then(|a| scale_op(a, scaling_factor))
        .ok_or(ScopeError::MathOverflow)?;

    if stdev_x_confidence_factor_scaled >= price_mantissa {
        Err(ScopeError::PriceNotValid)
    } else {
        Ok(())
    }
}

impl TryFrom<Decimal> for Price {
    type Error = ScopeError;

    fn try_from(sb_decimal: Decimal) -> std::result::Result<Self, Self::Error> {
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

mod switchboard {
    use rust_decimal::Decimal;
    use sha2::{Digest, Sha256};
    use solana_program::pubkey::Pubkey;
    use solana_program::clock::Clock;
    use std::cell::Ref;

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
        /// The timestamp at which this value was signed.
        pub timestamp: i64,
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

        /// The mean of the submissions needed for quorom size
        pub fn mean(&self) -> Option<Decimal> {
            if self.slot == 0 {
                return None;
            }
            Some(Decimal::from_i128_with_scale(self.mean, PRECISION))
        }

        /// The range of the submissions needed for quorom size
        pub fn range(&self) -> Option<Decimal> {
            if self.slot == 0 {
                return None;
            }
            Some(Decimal::from_i128_with_scale(self.range, PRECISION))
        }

        /// The minimum value of the submissions needed for quorom size
        pub fn min_value(&self) -> Option<Decimal> {
            if self.slot == 0 {
                return None;
            }
            Some(Decimal::from_i128_with_scale(self.min_value, PRECISION))
        }

        /// The maximum value of the submissions needed for quorom size
        pub fn max_value(&self) -> Option<Decimal> {
            if self.slot == 0 {
                return None;
            }
            Some(Decimal::from_i128_with_scale(self.max_value, PRECISION))
        }

        pub fn result_slot(&self) -> Option<u64> {
            if self.slot == 0 {
                return None;
            }
            Some(self.slot)
        }

        pub fn min_slot(&self) -> Option<u64> {
            if self.slot == 0 {
                return None;
            }
            Some(self.min_slot)
        }

        pub fn max_slot(&self) -> Option<u64> {
            if self.slot == 0 {
                return None;
            }
            Some(self.max_slot)
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
    pub struct OracleSubmission {
        /// The public key of the oracle that submitted this value.
        pub oracle: Pubkey,
        /// The slot at which this value was signed.
        pub slot: u64,
        /// The timestamp at which this value was signed.
        pub timestamp: i64,
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
        _padding1: [u8; 3],
        pub sample_size: u8,
        pub last_update_timestamp: i64,
        pub lut_slot: u64,
        pub ipfs_hash: [u8; 32], // deprecated
        pub result: CurrentResult,
        pub max_staleness: u32,
        _ebuf4: [u8; 20],
        _ebuf3: [u8; 24],
        _ebuf2: [u8; 256],
        _ebuf1: [u8; 512],
    }

    impl OracleSubmission {
        pub fn is_empty(&self) -> bool {
            self.slot == 0
        }

        pub fn value(&self) -> Decimal {
            Decimal::from_i128_with_scale(self.value, PRECISION)
        }
    }

    impl PullFeedAccountData {

        pub fn parse<'info>(
            data: Ref<'info, &mut [u8]>,
        ) -> Result<Ref<'info, Self>, OnDemandError> {
            if data.len() < Self::discriminator().len() {
                return Err(OnDemandError::InvalidDiscriminator);
            }

            let mut disc_bytes = [0u8; 8];
            disc_bytes.copy_from_slice(&data[..8]);
            if disc_bytes != Self::discriminator() {
                return Err(OnDemandError::InvalidDiscriminator);
            }

            Ok(Ref::map(data, |data: &&mut [u8]| {
                bytemuck::from_bytes(&data[8..std::mem::size_of::<Self>() + 8])
            }))
        }

        pub fn discriminator() -> [u8; 8] {
            [196, 27, 108, 196, 10, 215, 219, 40]
        }

        /// The median value of the submissions needed for quorom size
        pub fn value(&self) -> Option<Decimal> {
            self.result.value()
        }

        /// The standard deviation of the submissions needed for quorom size
        pub fn std_dev(&self) -> Option<Decimal> {
            self.result.std_dev()
        }

        /// The mean of the submissions needed for quorom size
        pub fn mean(&self) -> Option<Decimal> {
            self.result.mean()
        }

        /// The range of the submissions needed for quorom size
        pub fn range(&self) -> Option<Decimal> {
            self.result.range()
        }

        /// The minimum value of the submissions needed for quorom size
        pub fn min_value(&self) -> Option<Decimal> {
            self.result.min_value()
        }

        /// The maximum value of the submissions needed for quorom size
        pub fn max_value(&self) -> Option<Decimal> {
            self.result.max_value()
        }
    }
}
