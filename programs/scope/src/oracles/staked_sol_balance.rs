use anchor_lang::prelude::*;
use solana_program::{
    borsh0_10::try_from_slice_unchecked,
    stake::{self, state::StakeState},
};

use anchor_spl::token::spl_token::native_mint::DECIMALS as SOL_DECIMALS;

use crate::{warn, DatedPrice, Price, ScopeError, ScopeResult};

fn parse_stake_account(account_info: &AccountInfo) -> ScopeResult<StakeState> {
    // Validate account owner is the stake program
    if account_info.owner != &stake::program::ID {
        warn!("Staked Sol Balance Oracle: Account owner is not the stake program");
        return Err(ScopeError::UnexpectedAccount);
    }
    let stake_state =
        try_from_slice_unchecked::<StakeState>(&account_info.data.borrow()).map_err(|_| {
            warn!("Staked Sol Balance Oracle: Failed to deserialize stake state");
            ScopeError::UnexpectedAccount
        })?;
    Ok(stake_state)
}

fn validate_stake_account(account_info: &AccountInfo) -> ScopeResult<()> {
    let stake_state = parse_stake_account(account_info)?;
    match stake_state {
        StakeState::Stake(_, stake) => {
            if stake.delegation.deactivation_epoch != u64::MAX {
                warn!("Staked Sol Balance Oracle: Stake account is deactivating");
                return Err(ScopeError::UnexpectedAccount);
            }
            Ok(())
        }
        _ => {
            warn!("Staked Sol Balance Oracle: Stake account is not in delegated stake state");
            Err(ScopeError::UnexpectedAccount)
        }
    }
}

/// Get the staked SOL balance from a stake account as a price
///
/// The balance is returned with exp=9 (SOL decimals), meaning the value is in lamports.
/// We use `delegation.stake` to get the exact delegated amount, which excludes the
/// rent-exempt reserve and any extra unstaked lamports sent to the account.
/// Accounts that have started deactivating are rejected conservatively.
pub fn get_price(stake_account_info: &AccountInfo, clock: &Clock) -> Result<DatedPrice> {
    let stake_state = parse_stake_account(stake_account_info)?;
    let staked_lamports = match stake_state {
        StakeState::Stake(_, stake) => {
            if stake.delegation.deactivation_epoch != u64::MAX {
                warn!("Staked Sol Balance Oracle: Stake account is deactivating");
                return Err(ScopeError::PriceNotValid.into());
            }
            stake.delegation.stake
        }
        _ => {
            warn!("Staked Sol Balance Oracle: Stake account is not in delegated stake state");
            return Err(ScopeError::UnexpectedAccount.into());
        }
    };

    Ok(DatedPrice {
        price: Price {
            value: staked_lamports,
            exp: u64::from(SOL_DECIMALS),
        },
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp)
            .map_err(|_| ScopeError::BadTimestamp)?,
        ..Default::default()
    })
}

pub fn validate_account(stake_account: Option<&AccountInfo>) -> Result<()> {
    let Some(stake_account) = stake_account else {
        warn!("Staked Sol Balance Oracle: No stake account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };

    validate_stake_account(stake_account)?;

    Ok(())
}
