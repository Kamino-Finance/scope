#![allow(clippy::result_large_err)]

pub mod u64x64_math;

use anchor_lang::prelude::*;
use decimal_wad::rate::U128;

declare_id!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");

#[program]
pub mod perpetuals {}

// size = 896 (0x380), align = 0x8
#[account(zero_copy)]
pub struct LbPair {
    pub parameters_buff: [u32; 32 / 4],   // Size 32, align 4
    pub v_parameters_buff: [u64; 32 / 8], // Size 32, align 8
    pub bump_seed: [u8; 1],
    /// Bin step signer seed
    pub bin_step_seed: [u8; 2],
    /// Type of the pair
    pub pair_type: u8,
    /// Active bin id
    pub active_id: i32,
    /// Bin step. Represent the price increment / decrement.
    pub bin_step: u16,
    /// Status of the pair. Check PairStatus enum.
    pub status: u8,
    pub _padding1: [u8; 5],
    /// Token X mint
    pub token_x_mint: Pubkey,
    /// Token Y mint
    pub token_y_mint: Pubkey,
    /// LB token X vault
    pub reserve_x: Pubkey,
    /// LB token Y vault
    pub reserve_y: Pubkey,
    /// Uncollected protocol fee
    pub protocol_fee: [u64; 2], // Size 16, align 8
    /// Protocol fee owner,
    pub fee_owner: Pubkey,
    /// Farming reward information
    pub reward_infos_buffs: [[u64; 144 / 8]; 2], // (size = 144 (0x90), align = 0x8) * 2
    /// Oracle pubkey
    pub oracle: Pubkey,
    /// Packed initialized bin array state
    pub bin_array_bitmap: [u64; 16], // store default bin id from -512 to 511 (bin id from -35840 to 35840, price from 2.7e-16 to 3.6e15)
    /// Last time the pool fee parameter was updated
    pub last_updated_at: i64,
    /// Whitelisted wallet
    pub whitelisted_wallet: [Pubkey; 2],
    /// Base keypair. Only required for permission pair
    pub base_key: Pubkey,
    /// Slot to enable the pair. Only available for permission pair.
    pub activation_slot: u64,
    /// Last slot until pool remove max_swapped_amount for buying
    pub swap_cap_deactivate_slot: u64,
    /// Max X swapped amount user can swap from y to x between activation_slot and last_sloi
    pub max_swapped_amount: u64,
    /// Reserved space for future use
    pub _reserved: [u8; 64],
}

/// Calculate price based on the given bin id. Eg: 1.0001 ^ 5555. The returned value is in Q64.64
pub fn get_x64_price_from_id(active_id: i32, bin_step: u16) -> Option<U128> {
    // bin_step is in bps, convert to a fraction scaled by 64 bits (Q64x64).
    // Eg. If bin_step = 1, we get 0.0001 in Q64x64
    let step_f = (U128::from(bin_step) << 64) / 10_000;
    // Add 1 (scaled) to get the base
    let base = u64x64_math::ONE + step_f;
    u64x64_math::pow(base, active_id)
}
