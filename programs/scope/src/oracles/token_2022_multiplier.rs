//! Oracle for the effective multiplier of a Token-2022 mint's `ScaledUiAmount` extension
//! (e.g. corporate-action scaling of tokenized stocks). The mapping account is the mint and the
//! multiplier is read directly from it. The multiplier is set by the extension's authority — the
//! price is exactly as trustworthy as that authority.
//!
//! The effective multiplier changes without the mint account changing, when the wall clock
//! crosses a scheduled `new_multiplier`'s timestamp. The price is suspended in the blackout
//! window before that switch, so one carrying the old multiplier cannot outlive it. A resume
//! approves both the multiplier in effect and the scheduled one, but only takes hold once the
//! switch has activated: inside the window the next refresh suspends again.
//!
//! An update can also change the multiplier in effect immediately, or rewrite the `multiplier`
//! field while the scheduled switch is still ahead. The price is therefore suspended whenever the
//! multiplier it prices changes, not only around a scheduled switch.
//!
//! What the suspension cannot cover is the first price of an entry: with nothing recorded to
//! compare against, the first refresh takes the multiplier in effect and makes it the approved
//! one. It may differ from the one the mint held when the entry was configured, since a scheduled
//! switch might have already passed between configuration and the first refresh. The program does
//! not check that first value; the operator does, by looking at the published price before the
//! entry is used.

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022;

use crate::{
    compat::token_2022_scaled_ui_amount::{
        parse_scaled_ui_amount_multipliers, ScaledUiAmountMultipliers,
    },
    oracles::PriceRefreshOutcome,
    warn, DatedPrice, Price, ScopeError, ScopeResult,
};

/// How long before a scheduled multiplier activates the price stops being refreshed. Same period
/// as ChainlinkX's suspension window.
///
/// A price computed from the old multiplier can outlive a switch by at most one max age: the
/// first refresh that sees the change suspends instead of publishing, so the stored price ages
/// out. There is no lead time to rely on — an activation timestamp may be published already in
/// the past — so size max age against that exposure. The 24h blackout only helps for switches
/// announced further ahead than it.
pub const TIME_PERIOD_BEFORE_ACTIVATION_TO_SUSPEND_S: i64 = 24 * 60 * 60; // 24 hours

#[derive(Default, AnchorDeserialize, AnchorSerialize)]
pub struct Token2022MultiplierStoredData {
    pub suspended: bool,
    /// Raw bits of the multiplier the entry may price now. `None` on an entry that has priced
    /// nothing yet, which takes whatever the mint holds on its first refresh.
    pub approved_multiplier_bits: Option<u64>,
    /// Raw bits of the scheduled multiplier a resume covered in advance, priceable once the mint's
    /// schedule flips to it. Only a blackout suspension records one.
    pub approved_pending_bits: Option<u64>,
    /// The mint's switch timestamp when the entry suspended. `None` on an entry that is not
    /// suspended. The mint stores an `i64`, but this is a `u32` because the whole record has to
    /// fit the entry's 24 bytes: a switch announced past 2106 cannot be recorded, and the refresh
    /// rejects it.
    pub suspension_activation_timestamp: Option<u32>,
}

/// Shows the approved multipliers as the numbers they are, rather than the stored bits.
impl std::fmt::Debug for Token2022MultiplierStoredData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token2022MultiplierStoredData")
            .field("suspended", &self.suspended)
            .field(
                "approved_multiplier",
                &self.approved_multiplier_bits.map(f64::from_bits),
            )
            .field(
                "approved_pending",
                &self.approved_pending_bits.map(f64::from_bits),
            )
            .field(
                "suspension_activation_timestamp",
                &self.suspension_activation_timestamp,
            )
            .finish()
    }
}

// TODO(liviuc): move `GenericDataConvertible` out of `oracles::chainlink` into a shared utils
// module and implement it here, replacing these two methods.
impl Token2022MultiplierStoredData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize Token2022MultiplierStoredData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 24] {
        let mut buff = [0u8; 24];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize Token2022MultiplierStoredData");
        buff
    }
}

/// Checks the account is a token-2022-owned mint (a legacy SPL mint cannot carry the extension)
/// and parses its multipliers. Shared by the refresh and the config validation so they can't
/// drift apart.
fn checked_parse_multipliers(mint_account_info: &AccountInfo) -> Result<ScaledUiAmountMultipliers> {
    if *mint_account_info.owner != spl_token_2022::ID {
        warn!(
            "Scaled multiplier: mint {} is owned by {} but expected the token-2022 program {}",
            mint_account_info.key(),
            mint_account_info.owner,
            spl_token_2022::ID
        );
        return err!(ScopeError::WrongAccountOwner);
    }
    Ok(parse_scaled_ui_amount_multipliers(
        &mint_account_info.data.borrow(),
    )?)
}

/// Get the mint's currently effective scaled-ui-amount multiplier as a price.
///
/// NaN, infinite and negative multipliers are rejected by the `Price` conversion; a zero
/// multiplier is rejected by the refresh gate like every other zero price.
pub fn get_price(
    mint_account_info: &AccountInfo,
    dated_price: &DatedPrice,
    clock: &Clock,
) -> Result<PriceRefreshOutcome> {
    let multipliers = checked_parse_multipliers(mint_account_info)?;

    // A suspended price stays frozen until it is resumed: refusing to refresh leaves the stored
    // price untouched, so it ages out.
    let stored_data = Token2022MultiplierStoredData::from_generic_data(&dated_price.generic_data)?;
    if stored_data.suspended {
        warn!(
            "Scaled multiplier: mint {} is suspended, rejecting the refresh",
            mint_account_info.key()
        );
        return err!(ScopeError::PriceNotValid);
    }

    let effective_multiplier = multipliers.effective_multiplier(clock.unix_timestamp);
    let effective_bits = effective_multiplier.to_bits();

    // An entry that has priced nothing yet takes the multiplier in effect: the operator checks
    // the first published price before anything consumes the entry.
    let approved_bits = stored_data
        .approved_multiplier_bits
        .unwrap_or(effective_bits);

    let time_to_activation = multipliers
        .new_multiplier_effective_timestamp
        .saturating_sub(clock.unix_timestamp);
    // The switch timestamp identifies the schedule an approval is bound to, so it must be
    // recordable
    let switch_timestamp = u32::try_from(multipliers.new_multiplier_effective_timestamp)
        .map_err(|_| ScopeError::BadTimestamp)?;
    let suspend = |stored_data: Token2022MultiplierStoredData| {
        Ok(PriceRefreshOutcome::Suspended(
            stored_data.to_generic_data(),
        ))
    };

    // A switch to the value already in effect changes nothing, so it needs no blackout.
    let switch_changes_the_multiplier =
        multipliers.new_multiplier.to_bits() != multipliers.multiplier.to_bits();

    // Stop refreshing ahead of a scheduled switch so the stored price, computed from the old
    // multiplier, ages out before the new one takes effect rather than staying valid past it.
    // Only the price data changes here, not the price.
    if switch_changes_the_multiplier
        && (1..=TIME_PERIOD_BEFORE_ACTIVATION_TO_SUSPEND_S).contains(&time_to_activation)
    {
        // Approve the value about to take effect, and keep the current one approved so a switch
        // that is postponed or cancelled costs no second resume.
        let recorded = Token2022MultiplierStoredData {
            suspended: true,
            approved_multiplier_bits: Some(approved_bits),
            approved_pending_bits: Some(multipliers.new_multiplier.to_bits()),
            suspension_activation_timestamp: Some(switch_timestamp),
        };
        // A resume approves what is recorded here, so the log must name it.
        warn!(
            "Scaled multiplier: mint {} switches multiplier from {} to {} in {}s, suspending its price ({recorded:?})",
            mint_account_info.key(),
            effective_multiplier,
            multipliers.new_multiplier,
            time_to_activation
        );
        return suspend(recorded);
    }

    // Before the gate below, so a multiplier that cannot be priced is rejected rather than approved.
    let price = Price::try_from(effective_multiplier)?;

    // Any other change of the multiplier in effect needs a resume, whatever the schedule says.
    // A pending value was approved for one schedule, so the mint must still announce the same
    // switch timestamp.
    let effective_is_approved = effective_bits == approved_bits
        || (stored_data.approved_pending_bits == Some(effective_bits)
            && stored_data.suspension_activation_timestamp == Some(switch_timestamp));

    if !effective_is_approved {
        warn!(
            "Scaled multiplier: mint {} prices multiplier {} but {} is approved, suspending its price ({:?})",
            mint_account_info.key(),
            effective_multiplier,
            f64::from_bits(approved_bits),
            stored_data
        );
        return suspend(Token2022MultiplierStoredData {
            suspended: true,
            approved_multiplier_bits: Some(effective_bits),
            approved_pending_bits: None,
            suspension_activation_timestamp: Some(switch_timestamp),
        });
    }

    Ok(PriceRefreshOutcome::Updated(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp)
            .map_err(|_| ScopeError::BadTimestamp)?,
        generic_data: Token2022MultiplierStoredData {
            suspended: false,
            approved_multiplier_bits: Some(effective_bits),
            // Dropped here, so the authority cannot apply the same multiplier at some later date
            // and have it accepted without a new approval. Note: this only takes effect when a price is
            // written: while every refresh after the resume is discarded (frozen entry, failing
            // ref price check), the approval stays. Accepted trade-off, as the value was approved.
            approved_pending_bits: None,
            // The entry is running again, so no suspension is awaiting an approval.
            suspension_activation_timestamp: None,
        }
        .to_generic_data(),
    }))
}

/// Validate the oracle configuration: the mapped account must be a token-2022 mint whose
/// `ScaledUiAmount` extension parses. The multiplier itself is not checked here — the entry
/// adopts the one in effect on its first refresh, and any change after that suspends it.
pub fn validate_oracle_config(mint_account: Option<&AccountInfo>) -> Result<()> {
    let Some(mint_account) = mint_account else {
        warn!("Scaled multiplier: no mint account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    checked_parse_multipliers(mint_account)?;
    Ok(())
}
