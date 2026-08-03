//! Oracle for klend cToken exchange rate
//!
//! klend's `calculate_ctoken_exchange_rate` instruction does not refresh the reserve - it requires
//! the reserve to already be fresh in the current slot. So this oracle issues two CPIs in the same
//! transaction: first `refresh_reserves_batch` (skipping prices) to refresh the reserve, then
//! `calculate_ctoken_exchange_rate` to read the exchange rate from its return data. The rate is
//! converted to a scope Price representing the ratio of underlying tokens per cToken.

use anchor_lang::{prelude::*, InstructionData};
use borsh::BorshDeserialize;
use decimal_wad::decimal::Decimal;
use klend_itf::{ExchangeRateWithDecimals, Reserve};
use solana_program::program::get_return_data;

use crate::{utils::zero_copy_deserialize, warn, DatedPrice, Price, ScopeError, ScopeResult};

/// Get the cToken exchange rate from klend via CPI.
///
/// # Accounts
/// * `reserve` - The klend reserve account (base account from oracle mapping)
/// * `clock` - The clock sysvar
/// * `extra_accounts` - Iterator over extra accounts:
///   0. klend_program
///   1. lending_market
pub fn get_price<'a, 'b>(
    reserve: &AccountInfo<'a>,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> ScopeResult<DatedPrice>
where
    'a: 'b,
{
    // Read lending_market from the reserve before CPI (borrow dropped before invoke)
    let lending_market = {
        let reserve_state = zero_copy_deserialize::<Reserve>(reserve)?;
        reserve_state.lending_market
    };

    let klend_program = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;
    let lending_market_account = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;

    // Verify the supplied program account is actually klend before CPIing into it.
    if klend_program.key() != klend_itf::ID {
        warn!(
            "Unexpected klend program account: got {}, expected {}",
            klend_program.key(),
            klend_itf::ID
        );
        return Err(ScopeError::UnexpectedAccount);
    }

    // Verify the lending market account matches what the reserve expects
    if lending_market_account.key() != lending_market {
        warn!(
            "Lending market mismatch: reserve expects {}, got {}",
            lending_market,
            lending_market_account.key()
        );
        return Err(ScopeError::UnexpectedAccount);
    }

    // Both CPIs are fallible, but on-chain a klend revert aborts the whole transaction at the
    // syscall — control never returns here. So `invoke` only ever returns `Err` for a pre-syscall
    // failure in *our* code (e.g. an account still borrowed across the call), which is a bug; we
    // `expect` on it rather than papering over it with a soft error. The off-chain crank avoids
    // sending a tx that would abort by simulating each cToken refresh first and refreshing cToken
    // entries in their own single-entry transaction.

    // CPI 1: refresh the reserve (skipping prices). `refresh_reserves_batch` takes no named
    // accounts - it reads (reserve, lending_market) pairs from its remaining accounts - and a
    // single `skip_price_updates: bool` arg. `InstructionData::data()` emits the Anchor
    // discriminator followed by the borsh-encoded args.
    let refresh_ix = solana_program::instruction::Instruction {
        program_id: klend_itf::ID,
        accounts: vec![
            AccountMeta::new(reserve.key(), false),
            AccountMeta::new_readonly(lending_market, false),
        ],
        data: klend_itf::instruction::RefreshReservesBatch {
            skip_price_updates: true,
        }
        .data(),
    };
    solana_program::program::invoke(
        &refresh_ix,
        &[
            klend_program.clone(),
            reserve.clone(),
            lending_market_account.clone(),
        ],
    )
    .expect("refresh_reserves_batch invoke returned Err; a klend revert aborts the tx instead, so this is a pre-syscall bug (e.g. a held account borrow)");

    // CPI 2: read the exchange rate off the now-fresh reserve.
    let calculate_ix = solana_program::instruction::Instruction {
        program_id: klend_itf::ID,
        accounts: vec![AccountMeta::new_readonly(reserve.key(), false)],
        data: klend_itf::instruction::CalculateCtokenExchangeRate {}.data(),
    };
    solana_program::program::invoke(&calculate_ix, &[klend_program.clone(), reserve.clone()])
        .expect("calculate_ctoken_exchange_rate invoke returned Err; a klend revert aborts the tx instead, so this is a pre-syscall bug (e.g. a held account borrow)");

    // Read return data (set by the calculate ix)
    let (program_id, return_data) = get_return_data().ok_or_else(|| {
        warn!("No return data from klend calculate_ctoken_exchange_rate");
        ScopeError::KlendCTokenExchangeRateCPIError
    })?;

    if program_id != klend_itf::ID {
        warn!(
            "Return data from unexpected program: {} (expected {})",
            program_id,
            klend_itf::ID
        );
        return Err(ScopeError::KlendCTokenExchangeRateCPIError);
    }

    // On success klend returns the exchange rate directly as its CPI return data.
    let rate = ExchangeRateWithDecimals::try_from_slice(&return_data).map_err(|e| {
        warn!("Failed to deserialize klend return data: {:?}", e);
        ScopeError::KlendCTokenExchangeRateCPIError
    })?;

    // `exchange_rate_sf` is a U68F60 fixed-point value (60 fractional bits, so the real value is
    // `exchange_rate_sf / 2^60`). It is the amount of underlying-liquidity lamports that one whole
    // cToken redeems for, not yet a normalized rate. Adding mint_decimals to the price exponent
    // divides by 10^mint_decimals, turning those underlying lamports into whole underlying tokens
    // per cToken (klend's contract: rate = exchange_rate_sf / 10^mint_decimals).
    let exchange_rate = Decimal::from(rate.exchange_rate_sf) / (1u128 << 60);
    let mut price = Price::try_from(exchange_rate)?;
    price.exp += u64::from(rate.mint_decimals);

    Ok(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp)
            .map_err(|_| ScopeError::BadTimestamp)?,
        ..Default::default()
    })
}

/// Validate the oracle account configuration.
pub fn validate_account(reserve: Option<&AccountInfo>) -> ScopeResult<()> {
    let reserve = reserve.ok_or(ScopeError::UnexpectedAccount)?;

    if *reserve.owner != klend_itf::ID {
        warn!(
            "Reserve owner is {} but expected {}",
            reserve.owner,
            klend_itf::ID
        );
        return Err(ScopeError::WrongAccountOwner);
    }

    let reserve_state = zero_copy_deserialize::<Reserve>(reserve)?;

    // Reject a deprecated reserve at config time rather than only discovering it when the refresh
    // CPI reverts. A reserve whose schema `version` differs from klend's current `PROGRAM_VERSION`
    // is incompatible — klend rejects it with `ReserveDeprecated`, so it can never price.
    if reserve_state.version != u64::from(klend_itf::PROGRAM_VERSION) {
        warn!(
            "Reserve version is {} but expected {} (deprecated/incompatible reserve)",
            reserve_state.version,
            klend_itf::PROGRAM_VERSION
        );
        return Err(ScopeError::KlendReserveDeprecated);
    }

    Ok(())
}
