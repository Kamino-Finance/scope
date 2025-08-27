use crate::u64x64_math::{from_decimal, pow, to_decimal};
use anchor_lang::prelude::*;
use std::convert::TryInto;

pub const PRECISION: u128 = 1_000_000_000_000;

#[account]
pub struct Pair {
    // Bumps
    pub pair_bump: u8,
    pub lst_mint_bump: u8,

    // Tokens
    pub base_token_mint: Pubkey,
    pub base_mint_decimals: u8,
    pub lst_mint: Pubkey,
    pub lst_mint_decimals: u8,
    pub lst_symbol: String,

    // Yield
    pub interval_apr_rate: u64,
    pub seconds_per_interval: i32,

    pub initial_exchange_rate: u64,
    pub last_yield_change_exchange_rate: u64,
    pub last_yield_change_timestamp: i64,

    // The max sum of deposits this pair will accept.
    pub deposit_cap: u64,
    pub minimum_deposit: u64,

    // Fees (in basis points)
    pub stake_fee_bps: u16,
    pub swap_fee_bps: u16,
    pub withdraw_fee_bps: u16,
}

impl Pair {
    pub fn calculate_exchange_rate(&self, current_timestamp: i64) -> Option<u64> {
        if current_timestamp == self.last_yield_change_timestamp {
            return Some(self.last_yield_change_exchange_rate);
        }

        // Prevent timestamp manipulation by ensuring current_timestamp is greater than last_yield_change_timestamp
        if current_timestamp < self.last_yield_change_timestamp {
            return None;
        }

        let elapsed_time = current_timestamp.checked_sub(self.last_yield_change_timestamp)?;
        msg!("Elapsed time: {}", elapsed_time);

        // Convert i32 to i64 (infallible, so direct cast is fine)
        let seconds_per_interval_i64 = self.seconds_per_interval as i64;

        let interval_amounts = elapsed_time.checked_div(seconds_per_interval_i64)?;
        let remaining_seconds = elapsed_time.checked_rem(seconds_per_interval_i64)?;
        msg!("intervals: {}", interval_amounts);
        msg!("Remaining seconds: {}", remaining_seconds);

        // Convert u64 to u128 (infallible, so direct cast is fine)
        let interval_rate = self.interval_apr_rate as u128;
        msg!("Interval Rate: {}", interval_rate);

        // Convert interval_rate to fixed-point for the pow function
        let interval_rate_fp = from_decimal(interval_rate)?;

        // Use the pow function to calculate the compounded rate
        // Convert i64 to i32 with checked conversion
        let interval_amounts_i32 = i32::try_from(interval_amounts).ok()?;
        let compounded_rate_fp = pow(interval_rate_fp, interval_amounts_i32)?;

        // Convert back to decimal
        let compounded_rate = to_decimal(compounded_rate_fp)?;
        msg!("Compounded rate: {}", compounded_rate);

        // Calculate the linear yield for the remaining seconds
        // First subtract PRECISION to get just the yield portion
        let yield_portion = interval_rate.checked_sub(PRECISION)?;

        // Convert i64 to u128 with checked conversion
        let remaining_seconds_u128 = u128::try_from(remaining_seconds).ok()?;
        // Convert i32 to u128 with checked conversion
        let seconds_per_interval_u128 = u128::try_from(self.seconds_per_interval).ok()?;

        let linear_yield = yield_portion
            .checked_mul(remaining_seconds_u128)?
            .checked_div(seconds_per_interval_u128)?;
        msg!("Linear yield: {}", linear_yield);

        // Add the linear yield to the compounded rate
        let total_rate = compounded_rate.checked_add(linear_yield)?;
        msg!("Total rate: {}", total_rate);

        // Multiply the current exchange rate with the total rate
        // Convert u64 to u128 (infallible, so direct cast is fine)
        let last_yield_change_exchange_rate_u128 = self.last_yield_change_exchange_rate as u128;

        let new_exchange_rate = last_yield_change_exchange_rate_u128
            .checked_mul(total_rate)?
            .checked_div(PRECISION)?;

        // Scale the exchange rate appropriately
        // The exchange rate should be in the millions range (10^6)
        msg!("New exchange rate: {}", new_exchange_rate);

        // Use checked conversion instead of unchecked cast
        let new_exchange_rate_u64 = new_exchange_rate.try_into().ok()?;

        Some(new_exchange_rate_u64)
    }
}
