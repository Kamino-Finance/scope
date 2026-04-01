use anchor_lang::{prelude::*, solana_program::program_option::COption};
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::{warn, DatedPrice, Price, ScopeError, ScopeResult};

fn parse_token_account(account_info: &AccountInfo) -> ScopeResult<TokenAccount> {
    // Use InterfaceAccount::try_from for owner check + deserialization,
    // then extract the inner TokenAccount
    let interface_account =
        InterfaceAccount::<TokenAccount>::try_from(account_info).map_err(|_| {
            warn!("Spl Balance Oracle: Provided pubkey is not a valid SPL token account");
            ScopeError::UnexpectedAccount
        })?;
    Ok(interface_account.into_inner())
}

fn parse_mint(account_info: &AccountInfo) -> ScopeResult<Mint> {
    let interface_account = InterfaceAccount::<Mint>::try_from(account_info).map_err(|_| {
        warn!("Spl Balance Oracle: Failed to parse mint account");
        ScopeError::UnexpectedAccount
    })?;
    Ok(interface_account.into_inner())
}

/// Get the token balance from an SPL token account as a price
///
/// The balance is returned with exp=token_decimals, meaning the value
/// represents the number of whole tokens when divided by 10^exp.
pub fn get_price<'a, 'b>(
    token_account_info: &AccountInfo,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    let token_account = parse_token_account(token_account_info)?;

    if token_account.is_frozen() {
        warn!("Spl Balance Oracle: Token account is frozen, balance is inaccessible");
        return Err(ScopeError::PriceNotValid.into());
    }

    // Get mint account from extra accounts
    let mint_acc = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    // Validate mint matches token account's mint
    if mint_acc.key != &token_account.mint {
        warn!(
            "Spl Balance Oracle: Mint account mismatch. Expected {}, got {}",
            token_account.mint, mint_acc.key
        );
        return Err(ScopeError::UnexpectedAccount.into());
    }

    // Validate both accounts are owned by the same token program
    if token_account_info.owner != mint_acc.owner {
        warn!(
            "Spl Balance Oracle: Token program mismatch. Token account owner: {}, mint owner: {}",
            token_account_info.owner, mint_acc.owner
        );
        return Err(ScopeError::UnexpectedAccount.into());
    }

    let mint = parse_mint(mint_acc)?;

    // For native SOL token accounts, compute the effective balance from lamports
    // to handle the case where SOL was sent directly without calling sync_native.
    let balance = match token_account.is_native {
        COption::Some(rent_exempt_reserve) => token_account_info
            .lamports()
            .checked_sub(rent_exempt_reserve)
            .ok_or_else(|| {
                warn!(
                    "Spl Balance Oracle: Native token account lamports {} below reserve {}",
                    token_account_info.lamports(),
                    rent_exempt_reserve
                );
                ScopeError::PriceNotValid
            })?,
        COption::None => token_account.amount,
    };

    Ok(DatedPrice {
        price: Price {
            value: balance,
            exp: u64::from(mint.decimals),
        },
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp)
            .map_err(|_| ScopeError::BadTimestamp)?,
        ..Default::default()
    })
}

pub fn validate_account(token_account: Option<&AccountInfo>) -> Result<()> {
    let Some(token_account) = token_account else {
        warn!("No token account provided for SplBalance oracle");
        return err!(ScopeError::ExpectedPriceAccount);
    };

    // Try to deserialize to validate it's a valid token account
    parse_token_account(token_account)?;

    Ok(())
}
