use anchor_lang::prelude::*;

use crate::{
    oracles::{
        chainlink::{ChainlinkXPriceData, GenericDataConvertible},
        check_context,
        token_2022_multiplier::Token2022MultiplierStoredData,
        OracleType,
    },
    states::{Configuration, OracleMappings, OraclePrices, TokenMetadatas},
    utils::pdas::seeds,
    ScopeError,
};

#[derive(Accounts)]
#[instruction(token: u16, feed_name: String)]
pub struct ResumeSuspendedPrice<'info> {
    #[account(constraint = configuration.load()?.can_resume(&authority.key()) @ ScopeError::UnauthorizedResume)]
    pub authority: Signer<'info>,

    #[account(seeds = [seeds::CONFIG, feed_name.as_bytes()], bump, has_one = oracle_prices, has_one = oracle_mappings, has_one = tokens_metadata)]
    pub configuration: AccountLoader<'info, Configuration>,

    #[account(mut, has_one = oracle_mappings)]
    pub oracle_prices: AccountLoader<'info, OraclePrices>,

    pub oracle_mappings: AccountLoader<'info, OracleMappings>,
    pub tokens_metadata: AccountLoader<'info, TokenMetadatas>,
}

pub fn process(
    ctx: Context<ResumeSuspendedPrice>,
    token: u16,
    expected_price_data: [u8; 24],
) -> Result<()> {
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
    // Audit trail: record who resumed and whether it was the admin or the resume delegate.
    let authority = ctx.accounts.authority.key();
    let authority_role = if authority == ctx.accounts.configuration.load()?.admin {
        "admin"
    } else {
        "resume_authority"
    };
    msg!(
        "ResumeSuspendedPrice, token: {} ({}), authority: {} ({})",
        token,
        str_name,
        authority,
        authority_role
    );

    // Check that the token at entry_id is an oracle type that can suspend its price
    let price_type: OracleType = oracle_mappings.get_entry_type(entry_id)?;

    let dated_price = oracle_prices
        .prices
        .get_mut(entry_id)
        .ok_or(ScopeError::BadTokenNb)?;

    // A resume approves the suspension it was built for: the caller names the price data the
    // entry holds, so an approval collected for one suspension cannot be executed on a later one.
    // Naming the whole record rather than one of its fields covers the suspensions that agree on
    // a field, such as two multiplier changes applied with the same effective timestamp.
    require!(
        dated_price.generic_data == expected_price_data,
        ScopeError::ResumeStateMismatch
    );

    match price_type {
        OracleType::ChainlinkX => {
            // Parse existing price data
            let mut existing_price_data =
                ChainlinkXPriceData::from_generic_data(&dated_price.generic_data)?;
            msg!("Current deserialized price data: {:?}", existing_price_data);

            // Check that the price is currently suspended and resume its refresh
            if existing_price_data.suspended {
                // Resume the price refresh by setting suspended to false
                existing_price_data.suspended = false;
                // Set the observations timestamp to the current timestamp, such that only new
                // reports are able to refresh the price
                let clock = Clock::get()?;
                existing_price_data.observations_timestamp = clock
                    .unix_timestamp
                    .try_into()
                    .map_err(|_| ScopeError::OutOfRangeIntegralConversion)?;
                // Update the generic_data with the modified struct
                dated_price.generic_data = existing_price_data.to_generic_data();
            } else {
                // Kept for compatibility of the error code; new suspendable types use
                // PriceNotSuspended.
                return Err(ScopeError::ChainlinkXPriceNotSuspended.into());
            }
        }
        OracleType::Token2022Multiplier => {
            let mut stored_data =
                Token2022MultiplierStoredData::from_generic_data(&dated_price.generic_data)?;
            msg!("Current deserialized price data: {:?}", stored_data);

            require!(stored_data.suspended, ScopeError::PriceNotSuspended);
            stored_data.suspended = false;
            dated_price.generic_data = stored_data.to_generic_data();
        }
        _ => return err!(ScopeError::BadTokenType),
    }

    Ok(())
}
