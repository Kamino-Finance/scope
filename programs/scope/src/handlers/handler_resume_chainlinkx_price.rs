use anchor_lang::prelude::*;

use crate::{
    oracles::{
        chainlink::{ChainlinkXPriceData, GenericDataConvertible},
        check_context, OracleType,
    },
    states::{Configuration, OracleMappings, OraclePrices, TokenMetadatas},
    utils::pdas::seeds,
    ScopeError,
};

#[derive(Accounts)]
#[instruction(token: u16, feed_name: String)]
pub struct ResumeChainlinkXPrice<'info> {
    pub admin: Signer<'info>,

    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = admin, has_one = oracle_prices, has_one = oracle_mappings, has_one = tokens_metadata)]
    pub configuration: AccountLoader<'info, Configuration>,

    #[account(mut, has_one = oracle_mappings)]
    pub oracle_prices: AccountLoader<'info, OraclePrices>,

    pub oracle_mappings: AccountLoader<'info, OracleMappings>,
    pub tokens_metadata: AccountLoader<'info, TokenMetadatas>,
}

pub fn process(ctx: Context<ResumeChainlinkXPrice>, token: u16) -> Result<()> {
    check_context(&ctx)?;

    let entry_id: usize = token.into();

    let oracle_mappings = ctx.accounts.oracle_mappings.load()?;
    let mut oracle_prices = ctx.accounts.oracle_prices.load_mut()?;
    let tokens_metadata = ctx.accounts.tokens_metadata.load()?;
    let token_name = tokens_metadata
        .metadatas_array
        .get(entry_id)
        .ok_or(ScopeError::BadTokenNb)?
        .name;

    let str_name = std::str::from_utf8(&token_name).unwrap();
    msg!("ResumeChainlinkxPrice, token: {} ({})", token, str_name);

    // Check that the token at entry_id is a ChainlinkX oracle type
    let price_type_u8 = *oracle_mappings
        .price_types
        .get(entry_id)
        .ok_or(ScopeError::BadTokenNb)?;
    let price_type: OracleType = price_type_u8
        .try_into()
        .map_err(|_| ScopeError::BadTokenType)?;
    require!(
        price_type == OracleType::ChainlinkX,
        ScopeError::BadTokenType
    );

    let dated_price = oracle_prices
        .prices
        .get_mut(entry_id)
        .ok_or(ScopeError::BadTokenNb)?;

    // Parse existing price data
    let mut existing_price_data =
        ChainlinkXPriceData::from_generic_data(&dated_price.generic_data)?;
    msg!("Current deserialized price data: {:?}", existing_price_data);

    // Check that the price is currently suspended and resume its refresh
    if existing_price_data.suspended {
        // Resume the price refresh by setting suspended to false
        existing_price_data.suspended = false;
        // Set the observations timestamp to the current timestamp, such that only new reports
        // are able to refresh the price
        let clock = Clock::get()?;
        existing_price_data.observations_timestamp = clock
            .unix_timestamp
            .try_into()
            .map_err(|_| ScopeError::OutOfRangeIntegralConversion)?;
        // Update the generic_data with the modified struct
        dated_price.generic_data = existing_price_data.to_generic_data();
    } else {
        return Err(ScopeError::ChainlinkXPriceNotSuspended.into());
    }

    Ok(())
}
