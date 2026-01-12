use anchor_lang::prelude::*;
use anchor_spl::token::spl_token::state::Mint;
use decimal_wad::decimal::Decimal;
pub use jup_perp_itf as perpetuals;
pub use perpetuals::utils::{check_mint_pk, get_mint_pk};
use solana_program::program_pack::Pack;

use crate::{utils::account_deserialize, warn, DatedPrice, Result, ScopeError};
pub const POOL_VALUE_SCALE_DECIMALS: u8 = 6;

/// Gives the price of 1 JLP token in USD
///
/// Uses the AUM of the pool and the supply of the JLP token to compute the price
pub fn get_price_no_recompute<'a, 'b>(
    jup_pool_acc: &AccountInfo,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    let jup_pool_pk = jup_pool_acc.key;
    let jup_pool: perpetuals::Pool = account_deserialize(jup_pool_acc)?;

    let mint_acc = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    check_mint_pk(jup_pool_pk, mint_acc.key, jup_pool.lp_token_bump)
        .map_err(|_| ScopeError::UnexpectedAccount)?;

    let mint = {
        let mint_borrow = mint_acc.data.borrow();
        Mint::unpack(&mint_borrow)
    }?;

    let lp_value = jup_pool.aum_usd;
    let lp_token_supply = mint.supply;

    // This is a sanity check to make sure the mint is configured as expected
    // This allows to just divide the two values to get the price
    require_eq!(mint.decimals, POOL_VALUE_SCALE_DECIMALS);

    let price_dec = Decimal::from(lp_value) / lp_token_supply;
    let dated_price = DatedPrice {
        price: price_dec.into(),
        // TODO: find a way to get the last update time
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp).unwrap(),
        ..Default::default()
    };

    Ok(dated_price)
}

pub fn validate_jlp_pool(account: Option<&AccountInfo>) -> Result<()> {
    let Some(account) = account else {
        warn!("No jlp pool account provided");
        return err!(ScopeError::PriceNotValid);
    };
    let _jlp_pool: perpetuals::Pool = account_deserialize(account)?;
    Ok(())
}
