use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::{warn, DatedPrice, Price, ScopeError, ScopeResult};

fn validate_non_native_mint(account_info: &AccountInfo) -> ScopeResult<()> {
    if account_info.key == &anchor_spl::token::spl_token::native_mint::id()
        || account_info.key == &anchor_spl::token_2022::spl_token_2022::native_mint::id()
    {
        warn!(
            "Total Mint Supply Oracle: Native SOL mints are not supported because wrapped SOL \
             supply is not tracked in the mint's public supply field"
        );
        return Err(ScopeError::UnexpectedAccount);
    }

    Ok(())
}

fn parse_mint(account_info: &AccountInfo) -> ScopeResult<Mint> {
    validate_non_native_mint(account_info)?;

    let interface_account = InterfaceAccount::<Mint>::try_from(account_info).map_err(|_| {
        warn!("Total Mint Supply Oracle: Failed to parse mint account");
        ScopeError::UnexpectedAccount
    })?;
    Ok(interface_account.into_inner())
}

/// Get the total minted supply from an SPL mint account as a price.
///
/// The supply is returned with exp=mint_decimals, meaning the value represents the
/// number of whole tokens when divided by 10^exp.
///
/// Note: the native SOL mints are not supported because wrapped SOL supply is tracked
/// through token accounts and `sync_native`, not through the mint's public `supply` field.
///
/// For Token-2022 mints, this oracle reads only the canonical public base-mint `supply`
/// field and does not inspect or apply extension-specific semantics.
///
/// In particular, mints using ConfidentialMintBurn may have an effective supply that is not
/// fully reflected in the public base-mint state, and mints using InterestBearing or
/// ScaledUiAmount can have a UI-visible amount that diverges from the raw base amount.
/// This oracle intentionally returns the raw public `mint.supply`.
pub fn get_price(mint_account_info: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    let mint = parse_mint(mint_account_info)?;

    Ok(DatedPrice {
        price: Price {
            value: mint.supply,
            exp: u64::from(mint.decimals),
        },
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp)
            .map_err(|_| ScopeError::BadTimestamp)?,
        ..Default::default()
    })
}

pub fn validate_account(mint_account: Option<&AccountInfo>) -> Result<()> {
    let Some(mint_account) = mint_account else {
        warn!("Total Mint Supply Oracle: No mint account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };

    parse_mint(mint_account)?;

    Ok(())
}
