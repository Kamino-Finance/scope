use anchor_lang::prelude::*;

use super::canary;
use crate::{warn, DatedPrice, Price, ScopeError, ScopeResult};

/// CPI Canary `get_price` on the mapped feed, then require it to read exactly 0 or 1 at exp 0.
pub fn get_price<'a, 'b>(
    price_info: &AccountInfo<'a>,
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> Result<DatedPrice>
where
    'a: 'b,
{
    let dated_price = canary::get_price(price_info, clock, extra_accounts)?;
    validate_breaker_price(dated_price.price)?;
    Ok(dated_price)
}

fn validate_breaker_price(price: Price) -> ScopeResult<()> {
    if matches!(
        price,
        Price {
            value: 0 | 1,
            exp: 0
        }
    ) {
        return Ok(());
    }
    warn!(
        "Breaker price is {} at exp {}, not 0 or 1 at exp 0",
        price.value, price.exp
    );
    Err(ScopeError::PriceNotValid)
}
