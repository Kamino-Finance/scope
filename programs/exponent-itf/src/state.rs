//! Mirror of the Exponent tranching program's `ExponentTranchingMarket` account, reduced to the
//! fields Scope reads (unread spans are padding, klend-itf style). Field names, order and types
//! follow the program's source
//! (<https://github.com/exponent-finance/exponent-monorepo/blob/fb36c935200a4ef0348d7b8235a3517dfc54fd90/solana/programs/exponent_tranching/src/state/exponent_tranching_market/exponent_tranching_market.rs>);
//! `Number` mirrors `libraries/precise_number` of
//! <https://github.com/exponent-finance/exponent-core> @ `f250d0bf`.
//!
//! On-chain readers must go through [`MarketCpiConfig::from_account_data`]: borsh-deserializing
//! the full ~1.3KB account by value overflows an SBF stack frame (4KB). The typed
//! [`ExponentTranchingMarket`] is the layout definition and the off-chain/test builder.

// The explicit import wins over the prelude glob: `Result` here is std's two-parameter form, not
// anchor's alias.
use std::result::Result;

use anchor_lang::{prelude::*, Discriminator};

/// High precision number: an `spl_math::PreciseNumber` (little-endian `U256`) with 1e12 precision,
/// so the natural value is `raw / 10^12`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, AnchorDeserialize, AnchorSerialize)]
#[repr(C)]
pub struct Number(pub [u64; 4]);

/// `Number`'s fixed-point denominator (`spl_math::precise_number::ONE`).
pub const NUMBER_DENOMINATOR: u128 = 1_000_000_000_000;

impl Number {
    /// The raw 1e12-scaled value, or `None` if it exceeds `u128`.
    pub fn raw_u128(self) -> Option<u128> {
        let [low, high, upper_low, upper_high] = self.0;
        if upper_low != 0 || upper_high != 0 {
            return None;
        }
        Some(u128::from(low) | (u128::from(high) << 64))
    }

    pub fn is_zero(self) -> bool {
        self.0 == [0; 4]
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, AnchorDeserialize, AnchorSerialize)]
pub struct TranchingMarketRoles {
    pub admin: Vec<Pubkey>,
    pub sentinel: Vec<Pubkey>,
}

/// One account of a CPI the market makes into its SY program: an index into the market's address
/// lookup table plus the account's meta flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, AnchorDeserialize, AnchorSerialize)]
pub struct CpiInterfaceContext {
    pub alt_index: u8,
    pub is_signer: bool,
    pub is_writable: bool,
}

/// The account lists (as lookup-table indices) for the CPIs the market makes into its SY program.
/// Only `get_sy_state` (the first list) is mirrored; deserialization stops there, leaving the
/// remaining lists as unread trailing bytes.
#[derive(Clone, Debug, Default, Eq, PartialEq, AnchorDeserialize, AnchorSerialize)]
pub struct CpiAccounts {
    pub get_sy_state: Vec<CpiInterfaceContext>,
}

#[account]
#[derive(Debug, Eq, PartialEq)]
pub struct ExponentTranchingMarket {
    pub address_lookup_table: Pubkey,
    /// `sy_mint`
    pub _padding0: [u8; 32],
    pub sy_program: Pubkey,
    /// `token_sy_escrow`, `mint_lp_senior`, `mint_lp_junior`, `self_address`
    pub _padding1: [u8; 128],
    pub return_model_storage: Pubkey,
    /// `signer_bump` through the reserved return-model region: bump seeds, `seed_id`,
    /// `status_flags`, `market_state`, financials, tranche supply/asset states, risk and fee
    /// configs, `last_updated_slot`, reserved curve params — all fixed-size, none read by Scope.
    pub _padding2: [u8; 1017],
    pub roles: TranchingMarketRoles,
    pub sy_cpi_accounts: CpiAccounts,
}

// Not derivable: `Default` for arrays stops at 32 elements.
impl Default for ExponentTranchingMarket {
    fn default() -> Self {
        Self {
            address_lookup_table: Pubkey::default(),
            _padding0: [0; 32],
            sy_program: Pubkey::default(),
            _padding1: [0; 128],
            return_model_storage: Pubkey::default(),
            _padding2: [0; 1017],
            roles: TranchingMarketRoles::default(),
            sy_cpi_accounts: CpiAccounts::default(),
        }
    }
}

/// The market's `update_market` CPI account configuration: everything Scope needs to build and
/// validate the CPI, extracted from the market account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketCpiConfig {
    pub address_lookup_table: Pubkey,
    pub sy_program: Pubkey,
    pub return_model_storage: Pubkey,
    pub get_sy_state: Vec<CpiInterfaceContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketReadError {
    /// The data does not start with `ExponentTranchingMarket`'s discriminator.
    Discriminator,
    /// The data is too short for the market layout, or its dynamic tail does not parse.
    Layout,
}

/// Offsets of the fields [`MarketCpiConfig`] reads within the account body (after the 8-byte
/// discriminator), and the size of the market's fixed part (everything before the dynamic
/// `roles`). Pinned against the typed mirror by `cpi_config_reader_matches_the_typed_mirror`.
const ADDRESS_LOOKUP_TABLE_OFFSET: usize = 0;
const SY_PROGRAM_OFFSET: usize = 64;
const RETURN_MODEL_STORAGE_OFFSET: usize = 224;
const MARKET_FIXED_SIZE: usize = 1273;

impl MarketCpiConfig {
    /// Extract the CPI config from a market account's data without materializing the ~1.3KB
    /// account on the stack (which overflows an SBF stack frame): read the three pubkeys in
    /// place, skip the dynamic `roles`, and deserialize only the small `get_sy_state` list.
    pub fn from_account_data(data: &[u8]) -> Result<Self, MarketReadError> {
        let body = data
            .strip_prefix(ExponentTranchingMarket::DISCRIMINATOR.as_slice())
            .ok_or(MarketReadError::Discriminator)?;
        let pubkey_at = |offset: usize| -> Result<Pubkey, MarketReadError> {
            Ok(Pubkey::new_from_array(
                body.get(offset..offset + 32)
                    .ok_or(MarketReadError::Layout)?
                    .try_into()
                    .unwrap(),
            ))
        };
        // Skip the two `roles` vecs (borsh: a u32 length, then 32 bytes per pubkey).
        let mut rest = body
            .get(MARKET_FIXED_SIZE..)
            .ok_or(MarketReadError::Layout)?;
        for _ in 0..2 {
            let len_bytes = rest.get(..4).ok_or(MarketReadError::Layout)?;
            let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
            rest = rest.get(4 + len * 32..).ok_or(MarketReadError::Layout)?;
        }
        let get_sy_state = Vec::<CpiInterfaceContext>::deserialize(&mut rest)
            .map_err(|_| MarketReadError::Layout)?;
        Ok(Self {
            address_lookup_table: pubkey_at(ADDRESS_LOOKUP_TABLE_OFFSET)?,
            sy_program: pubkey_at(SY_PROGRAM_OFFSET)?,
            return_model_storage: pubkey_at(RETURN_MODEL_STORAGE_OFFSET)?,
            get_sy_state,
        })
    }
}

/// Offsets of `tranche_supply_state.total_{senior,junior}_lp_supply` within the account body
/// (after the 8-byte discriminator), inside the region [`ExponentTranchingMarket`] mirrors as
/// padding. Pinned by `lp_supplies_reader_reads_the_pinned_offsets`.
pub const TOTAL_SENIOR_LP_SUPPLY_OFFSET: usize = 580;
pub const TOTAL_JUNIOR_LP_SUPPLY_OFFSET: usize = 588;

/// The tranches' total LP supplies, read from the market account. Only trustworthy AFTER an
/// `update_market` CPI in the same transaction: the sync can materialize pending protocol-fee LP
/// shares into the totals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrancheLpSupplies {
    pub senior: u64,
    pub junior: u64,
}

impl TrancheLpSupplies {
    pub fn from_account_data(data: &[u8]) -> Result<Self, MarketReadError> {
        let body = data
            .strip_prefix(ExponentTranchingMarket::DISCRIMINATOR.as_slice())
            .ok_or(MarketReadError::Discriminator)?;
        let supply_at = |offset: usize| -> Result<u64, MarketReadError> {
            Ok(u64::from_le_bytes(
                body.get(offset..offset + 8)
                    .ok_or(MarketReadError::Layout)?
                    .try_into()
                    .unwrap(),
            ))
        };
        Ok(Self {
            senior: supply_at(TOTAL_SENIOR_LP_SUPPLY_OFFSET)?,
            junior: supply_at(TOTAL_JUNIOR_LP_SUPPLY_OFFSET)?,
        })
    }
}

/// `update_market`'s CPI return data. Layout (424 bytes) verified against mainnet return data; the
/// program's own field names are not published, so these names are ours.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, AnchorDeserialize, AnchorSerialize)]
pub struct UpdateMarketReturnData {
    pub market: Pubkey,
    pub sy_exchange_rate: Number,
    pub senior_raw_nav: Number,
    pub junior_raw_nav: Number,
    pub senior_effective_nav: Number,
    pub junior_effective_nav: Number,
    pub senior_loss: Number,
    pub junior_loss: Number,
    pub senior_premium: Number,
    pub junior_premium: Number,
    pub utilization: Number,
    pub senior_lp_price_net_asset: Number,
    pub junior_lp_price_net_asset: Number,
    pub timestamp: i64,
}
