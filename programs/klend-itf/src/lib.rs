// Anchor's `#[program]` handlers return `Result<_, anchor_lang::error::Error>`, which is large;
// matches the other interface crates (e.g. jup-perp-itf).
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;

pub mod state;

pub use state::*;

/// klend's current `Reserve`/`LendingMarket` schema version. A reserve whose `version` field
/// differs is deprecated and incompatible — klend itself rejects it with `ReserveDeprecated`.
/// Mirrors `PROGRAM_VERSION` in the klend program; must be bumped in lockstep with it.
pub const PROGRAM_VERSION: u8 = 1;

#[cfg(feature = "staging")]
declare_id!("SLendK7ySfcEzyaFqy93gDnD3RtrpXJcnRwb6zFHJSh");

#[cfg(not(feature = "staging"))]
declare_id!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");

/// Minimal Anchor interface to the klend instructions Scope CPIs into. The handler bodies are
/// `unimplemented!()` — we only need Anchor to generate each instruction's 8-byte discriminator and
/// borsh arg (de)serialization (the generated `instruction` module), which Scope uses to build the
/// CPI. Each fn name must match klend's exactly: the discriminator is `sha256("global:<fn_name>")`.
#[program]
pub mod kamino_lending {
    use super::*;

    #[allow(unused_variables)]
    pub fn refresh_reserves_batch(
        ctx: Context<RefreshReservesBatch>,
        skip_price_updates: bool,
    ) -> Result<()> {
        unimplemented!("klend-itf is just an interface")
    }

    #[allow(unused_variables)]
    pub fn calculate_ctoken_exchange_rate(
        ctx: Context<CalculateCtokenExchangeRate>,
    ) -> Result<ExchangeRateWithDecimals> {
        unimplemented!("klend-itf is just an interface")
    }
}

/// `refresh_reserves_batch` reads (reserve, lending_market) pairs from its remaining accounts and
/// takes no named accounts.
#[derive(Accounts)]
pub struct RefreshReservesBatch {}

#[derive(Accounts)]
pub struct CalculateCtokenExchangeRate<'info> {
    /// CHECK: interface only; the klend reserve to read the exchange rate from.
    pub reserve: AccountInfo<'info>,
}
