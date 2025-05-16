#![allow(clippy::result_large_err)] //Needed because we can't change Anchor result type
pub mod errors;
pub mod oracles;
pub mod program_id;
pub mod states;
pub mod utils;

mod handlers;

// Local use
use std::convert::TryInto;

pub use anchor_lang;
use anchor_lang::prelude::*;
pub use handler_update_token_metadata::UpdateTokenMetadataMode;
use handlers::*;
pub use num_enum;
use program_id::PROGRAM_ID;
pub use whirlpool;
#[cfg(feature = "yvaults")]
pub use yvaults;

pub use crate::{errors::*, states::*, utils::scope_chain};

declare_id!(PROGRAM_ID);

// Note: Need to be directly integer value to not confuse the IDL generator
pub const MAX_ENTRIES_U16: u16 = 512;
// Note: Need to be directly integer value to not confuse the IDL generator
pub const MAX_ENTRIES: usize = 512;
pub const VALUE_BYTE_ARRAY_LEN: usize = 32;

#[program]
pub mod scope {

    use super::*;

    pub fn initialize(ctx: Context<Initialize>, feed_name: String) -> Result<()> {
        handler_initialize::process(ctx, feed_name)
    }

    pub fn refresh_price_list<'info>(
        ctx: Context<'_, '_, '_, 'info, RefreshList<'info>>,
        tokens: Vec<u16>,
    ) -> Result<()> {
        handler_refresh_prices::refresh_price_list(ctx, &tokens)
    }

    pub fn refresh_chainlink_price<'info>(
        ctx: Context<'_, '_, '_, 'info, RefreshChainlinkPrice<'info>>,
        token: u16,
        serialized_chainlink_report: Vec<u8>,
    ) -> Result<()> {
        handler_refresh_chainlink_price::refresh_chainlink_price(
            ctx,
            token,
            serialized_chainlink_report,
        )
    }

    /// IMPORTANT: we assume the tokens passed in to this ix are in the same order in which
    /// they are found in the message payload. Thus, we rely on the client to do this work
    pub fn refresh_pyth_lazer_price<'info>(
        ctx: Context<'_, '_, '_, 'info, RefreshPythLazerPrice<'info>>,
        tokens: Vec<u16>,
        serialized_pyth_message: Vec<u8>,
        ed25519_instruction_index: u16,
    ) -> Result<()> {
        handler_refresh_pyth_lazer_price::refresh_pyth_lazer_price(
            ctx,
            &tokens,
            serialized_pyth_message,
            ed25519_instruction_index,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_mapping(
        ctx: Context<UpdateOracleMapping>,
        token: u16,
        price_type: u8,
        twap_enabled: bool,
        twap_source: u16,
        ref_price_index: u16,
        feed_name: String,
        generic_data: [u8; 20],
    ) -> Result<()> {
        let token: usize = token
            .try_into()
            .map_err(|_| ScopeError::OutOfRangeIntegralConversion)?;
        let _feed_name = feed_name;
        handler_update_mapping::process(
            ctx,
            token,
            price_type,
            twap_enabled,
            twap_source,
            ref_price_index,
            &generic_data,
        )
    }

    pub fn reset_twap(ctx: Context<ResetTwap>, token: u64, feed_name: String) -> Result<()> {
        let entry_id: usize = token
            .try_into()
            .map_err(|_| ScopeError::OutOfRangeIntegralConversion)?;
        handler_reset_twap::process(ctx, entry_id, feed_name)
    }

    pub fn update_token_metadata(
        ctx: Context<UpdateTokensMetadata>,
        index: u64,
        mode: u64,
        feed_name: String,
        value: Vec<u8>,
    ) -> Result<()> {
        msg!(
            "update_token_metadata index {} mode {} feed_name {}",
            index,
            mode,
            feed_name
        );
        let index: usize = index
            .try_into()
            .map_err(|_| ScopeError::OutOfRangeIntegralConversion)?;
        handler_update_token_metadata::process(ctx, index, mode, value, feed_name)
    }

    pub fn set_admin_cached(
        ctx: Context<SetAdminCached>,
        new_admin: Pubkey,
        feed_name: String,
    ) -> Result<()> {
        handler_set_admin_cached::process(ctx, new_admin, feed_name)
    }

    pub fn approve_admin_cached(ctx: Context<ApproveAdminCached>, feed_name: String) -> Result<()> {
        handler_approve_admin_cached::process(ctx, feed_name)
    }

    pub fn create_mint_map(
        ctx: Context<CreateMintMap>,
        seed_pk: Pubkey,
        seed_id: u64,
        bump: u8,
        scope_chains: Vec<[u16; 4]>,
    ) -> Result<()> {
        handler_create_mint_map::process(ctx, seed_pk, seed_id, bump, scope_chains)
    }

    pub fn close_mint_map(ctx: Context<CloseMintMap>) -> Result<()> {
        handler_close_mint_map::process(ctx)
    }
}
