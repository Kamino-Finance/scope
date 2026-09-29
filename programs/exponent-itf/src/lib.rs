// Anchor's `#[program]` handlers return `Result<_, anchor_lang::error::Error>`, which is large;
// matches the other interface crates (e.g. klend-itf).
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;
use solana_program::pubkey;

pub mod state;

pub use state::*;

declare_id!("XPTrnchoawiUc9iYJrpfchS8vgr8Y5X2QGBdHPXukty");

/// Seed of the tranching program's event authority PDA, which anchor's `event_cpi` convention
/// requires as an `update_market` account.
pub const EVENT_AUTHORITY_SEED: &[u8] = b"__event_authority";

/// The event authority PDA: `find_program_address([EVENT_AUTHORITY_SEED], &ID)`.
pub const EVENT_AUTHORITY: Pubkey = pubkey!("3mBi7DRWMdTdDghA1cVLrwDKAgDo7UTDWoeik4GkXCsf");

/// Minimal Anchor interface to the Exponent tranching instruction Scope CPIs into. The handler
/// body is `unimplemented!()` — we only need Anchor to generate the instruction's 8-byte
/// discriminator (the generated `instruction` module), which Scope uses to build the CPI. The fn
/// name must match Exponent's exactly: the discriminator is `sha256("global:<fn_name>")`.
#[program]
pub mod exponent_tranching {
    use super::*;

    #[allow(unused_variables)]
    pub fn update_market(ctx: Context<UpdateMarket>) -> Result<UpdateMarketReturnData> {
        unimplemented!("exponent-itf is just an interface")
    }
}

/// `update_market`'s fixed accounts; the market's `get_sy_state` CPI accounts (resolved through
/// its lookup table) follow as remaining accounts.
#[derive(Accounts)]
pub struct UpdateMarket<'info> {
    /// CHECK: interface only; the tranching market to sync and read.
    #[account(mut)]
    pub market: AccountInfo<'info>,
    /// CHECK: interface only; the market's return-model storage.
    #[account(mut)]
    pub return_model_storage: AccountInfo<'info>,
    /// CHECK: interface only; the market's address lookup table.
    pub address_lookup_table: AccountInfo<'info>,
    /// CHECK: interface only; the market's SY program.
    pub sy_program: AccountInfo<'info>,
    /// CHECK: interface only; the tranching program's `__event_authority` PDA.
    pub event_authority: AccountInfo<'info>,
    /// CHECK: interface only; the tranching program itself.
    pub program: AccountInfo<'info>,
}
