use anchor_lang::{
    prelude::*,
    solana_program::program::{get_return_data, invoke},
};
use canary_itf::{accounts::PriceFeed, PriceData};

use crate::{
    utils::{math, price_impl::u128_fixed_point_to_price, zero_copy_deserialize},
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

/// Refresh and mapping both reject exp above 18: Canary publishes at wad exp 18, and digits
/// past 1e-18 are unrepresentable downstream. Feed exp is immutable after `create_feed`, so
/// the refresh-time check is defense-in-depth, not a mutability guard.
const MAX_CANARY_EXP: u8 = 18;

/// Convert Canary `(value, exp)` into Scope `Price` (floors digits `u64` cannot hold).
fn scope_price_from_canary(data: &PriceData) -> ScopeResult<Price> {
    if data.exp > MAX_CANARY_EXP {
        warn!("Canary price exp {} is above {}", data.exp, MAX_CANARY_EXP);
        return Err(ScopeError::CanaryFeedExpTooLarge);
    }
    u128_fixed_point_to_price(data.value, data.exp)
}

/// CPI Canary `get_price` on the mapped feed account. Freshness is Scope `max_age` on the
/// returned `unix_timestamp`. `Live` is not freshness.
pub fn get_price<'a, 'b>(
    price_info: &AccountInfo<'a>,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    require_keys_eq!(
        *price_info.owner,
        canary_itf::ID,
        ScopeError::WrongAccountOwner
    );
    let canary_program = extra_accounts
        .next()
        .ok_or(ScopeError::AccountsAndTokenMismatch)?;
    require_keys_eq!(
        *canary_program.key,
        canary_itf::ID,
        ScopeError::UnexpectedAccount
    );

    let canary_price = fetch_price_data(price_info, canary_program)?;
    let price = scope_price_from_canary(&canary_price)?;
    let unix_timestamp = canary_price
        .unix_timestamp
        .min(u64::try_from(clock.unix_timestamp).map_err(|_| ScopeError::BadTimestamp)?);
    let last_updated_slot = math::estimate_slot_update_from_ts(clock, unix_timestamp);
    Ok(DatedPrice {
        price,
        unix_timestamp,
        last_updated_slot,
        generic_data: [0u8; 24],
    })
}

/// CPI Canary `get_price` and decode `PriceData`. Off-chain preflight simulates the refresh
/// (`ExecutionMode::Cpi`); this path is VM-only.
fn fetch_price_data<'a>(
    price_info: &AccountInfo<'a>,
    canary_program: &AccountInfo<'a>,
) -> Result<PriceData> {
    let ix = canary_itf::get_price(&canary_itf::ID, price_info.key);
    // Same as klend ctoken: a callee revert aborts the tx at the syscall. `Err` here is a
    // pre-syscall Scope bug (e.g. a held borrow).
    invoke(&ix, &[price_info.clone(), canary_program.clone()]).expect(
        "get_price invoke returned Err; a canary revert aborts the tx instead, so this is a pre-syscall bug (e.g. a held account borrow)",
    );

    let Some((return_program_id, return_data)) = get_return_data() else {
        warn!("Canary get_price returned no return data");
        return err!(ScopeError::CanaryPriceCPIError);
    };
    require_keys_eq!(
        return_program_id,
        canary_itf::ID,
        ScopeError::CanaryPriceCPIError
    );
    Ok(PriceData::try_from_slice(&return_data).map_err(|_| {
        warn!("Canary get_price return data did not decode as PriceData");
        ScopeError::CanaryPriceCPIError
    })?)
}

pub fn validate_price_account(price_account: Option<&AccountInfo>) -> Result<()> {
    let Some(price_account) = price_account else {
        warn!("No Canary price account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    require_keys_eq!(
        *price_account.owner,
        canary_itf::ID,
        ScopeError::WrongAccountOwner
    );
    if price_account.data_len() < PriceFeed::ACCOUNT_LEN {
        warn!(
            "Canary price feed {} is too short: {} bytes",
            price_account.key(),
            price_account.data_len()
        );
        return err!(ScopeError::UnableToDeserializeAccount);
    }
    let feed = zero_copy_deserialize::<PriceFeed>(price_account)?;
    if feed.exp > MAX_CANARY_EXP {
        warn!(
            "Canary feed {} exp {} is above {}",
            price_account.key(),
            feed.exp,
            MAX_CANARY_EXP
        );
        return err!(ScopeError::CanaryFeedExpTooLarge);
    }
    Ok(())
}
