//! Oracle for the effective multiplier of a Token-2022 mint's `ScaledUiAmount` extension
//! (e.g. corporate-action scaling of tokenized stocks). The mapping account is the mint and the
//! multiplier is read directly from it. The multiplier is set by the extension's authority — the
//! price is exactly as trustworthy as that authority.
//!
//! The entry may price a multiplier only while it is *approved*: named by an operator in a
//! resume, adopted by the entry on its first refresh, or, where an auto approval threshold is
//! configured, within that threshold of the reference. Anything else suspends the entry.
//!
//! The mint announces a *switch* by holding a `new_multiplier` and the
//! `new_multiplier_effective_timestamp` it takes over at. The effective multiplier therefore
//! changes without the mint account changing, when the wall clock crosses that timestamp. The
//! price is suspended in the blackout window before a switch, so one carrying the old multiplier
//! cannot outlive it. A resume names the multiplier it approves and leaves the entry suspended:
//! the price comes back on the first refresh that finds that multiplier, or one within the
//! threshold of it, in effect, which for a switch is once it has activated.
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
    utils::consts::{AUTO_APPROVAL_ANCHOR_PERIOD_S, FULL_BPS, MAX_DAILY_AUTO_APPROVAL_BPS},
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

/// Per-entry configuration, stored in the mapping's generic data.
#[derive(Debug, Default, PartialEq, Eq, AnchorDeserialize, AnchorSerialize)]
pub struct Token2022MultiplierMappingData {
    /// A multiplier within this many bps of the approved one is published without a resume, and
    /// a scheduled switch within it triggers no blackout. Zero means every change suspends.
    ///
    /// The reference it is measured against is replaced every 24h, on a fixed period rather than
    /// a rolling window. The last value published before the boundary can sit this far below the
    /// old reference and the first value after it, this far above, becomes the new one, so two
    /// consecutive published values can be about three times this apart, within seconds.
    ///
    /// A mapping update that only changes this value does not reset the entry, so the approved
    /// multiplier survives it: the update must not approve whatever the mint holds at that moment.
    pub daily_auto_approval_bps: u16,
}

impl Token2022MultiplierMappingData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            msg!("Failed to deserialize Token2022MultiplierMappingData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize Token2022MultiplierMappingData");
        buff
    }
}

/// Whether `multiplier` is within `daily_auto_approval_bps` of `approved`. A zero threshold auto
/// approves nothing, so only the exact approved value passes.
fn is_auto_approved(multiplier: f64, approved: f64, daily_auto_approval_bps: u16) -> bool {
    // The ratio is computed first to avoid an overflow.
    daily_auto_approval_bps > 0
        && multiplier.is_finite()
        && approved.is_finite()
        && (multiplier - approved).abs()
            <= approved.abs() * (f64::from(daily_auto_approval_bps) / f64::from(FULL_BPS))
}

/// Whether the multiplier in effect may be published against `approved`: the same value, or one
/// within `daily_auto_approval_bps` of it.
fn multiplier_is_accepted(multiplier: f64, approved: f64, daily_auto_approval_bps: u16) -> bool {
    multiplier.to_bits() == approved.to_bits()
        || is_auto_approved(multiplier, approved, daily_auto_approval_bps)
}

#[derive(Default, AnchorDeserialize, AnchorSerialize)]
pub struct Token2022MultiplierStoredData {
    pub suspended: bool,
    /// Raw bits of one multiplier, whose meaning is based on `suspended`:
    /// - while suspended: the value a resume approved, and the price comes back only once the mint
    ///   has it, or a multiplier within the threshold of it;
    /// - when running: the reference auto approval measures a change against.
    ///
    /// `None` on an entry that has priced nothing yet, which takes whatever the mint holds on its
    /// first refresh.
    pub approved_multiplier_bits: Option<u64>,
    /// One timestamp, whose meaning is based on `suspended`:
    /// - while suspended: the activation timestamp the mint announced for the switch the entry
    ///   suspended on;
    /// - when running: when the reference was set, which is when its 24h period started.
    ///
    /// `None` when there is neither.
    pub timestamp: Option<u64>,
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
            .field("timestamp", &self.timestamp)
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
    mapping_generic_data: &[u8; 20],
    clock: &Clock,
) -> Result<PriceRefreshOutcome> {
    let multipliers = checked_parse_multipliers(mint_account_info)?;
    let daily_auto_approval_bps =
        Token2022MultiplierMappingData::from_generic_data(mapping_generic_data)?
            .daily_auto_approval_bps;
    let stored_data = Token2022MultiplierStoredData::from_generic_data(&dated_price.generic_data)?;

    let effective_multiplier = multipliers.effective_multiplier(clock.unix_timestamp);
    let effective_bits = effective_multiplier.to_bits();

    // A suspended price stays frozen until a resume approves the multiplier the mint has in
    // effect, or one within the threshold of it. Refusing to refresh leaves the stored price
    // untouched, so it ages out.
    if stored_data.suspended {
        let lifts_the_suspension = match stored_data.approved_multiplier_bits {
            Some(approved_bits) => multiplier_is_accepted(
                effective_multiplier,
                f64::from_bits(approved_bits),
                daily_auto_approval_bps,
            ),
            None => false,
        };
        if !lifts_the_suspension {
            warn!(
                "Scaled multiplier: mint {} is suspended and prices {}, {:?} is approved, rejecting the refresh",
                mint_account_info.key(),
                effective_multiplier,
                stored_data.approved_multiplier_bits.map(f64::from_bits)
            );
            return err!(ScopeError::PriceNotValid);
        }
    }

    // The reference the multiplier in effect is judged against: the stored one, or the multiplier
    // in effect on an entry that has published nothing yet.
    let approved_bits = stored_data
        .approved_multiplier_bits
        .unwrap_or(effective_bits);
    let approved_multiplier = f64::from_bits(approved_bits);
    // A suspended entry was judged by the gate above on this same check.
    let effective_is_approved = stored_data.suspended
        || multiplier_is_accepted(
            effective_multiplier,
            approved_multiplier,
            daily_auto_approval_bps,
        );

    let now = u64::try_from(clock.unix_timestamp).map_err(|_| ScopeError::BadTimestamp)?;
    let time_to_activation = multipliers
        .new_multiplier_effective_timestamp
        .saturating_sub(clock.unix_timestamp);
    // The switch timestamp is recorded for the operator, so it must be recordable
    let switch_timestamp = u64::try_from(multipliers.new_multiplier_effective_timestamp)
        .map_err(|_| ScopeError::BadTimestamp)?;
    let suspend = |stored_data: Token2022MultiplierStoredData| {
        Ok(PriceRefreshOutcome::Suspended(
            stored_data.to_generic_data(),
        ))
    };

    // On a suspended entry the timestamp is the switch it suspended on, not a period start.
    let period_started = stored_data.timestamp.filter(|_| !stored_data.suspended);
    let period_is_over = period_started.map_or(false, |started| {
        now.saturating_sub(started) >= AUTO_APPROVAL_ANCHOR_PERIOD_S
    });

    // The reference this refresh leaves behind. It rotates onto the multiplier in effect once the
    // period is over, and only onto one the current reference approves.
    let rotates = effective_is_approved && period_is_over;
    let reference_bits = if rotates {
        effective_bits
    } else {
        approved_bits
    };

    // A switch to the value already in effect changes nothing, so it needs no blackout. Neither
    // does one within the threshold of the reference.
    let switch_changes_the_multiplier = multipliers.new_multiplier.to_bits()
        != multipliers.multiplier.to_bits()
        && !is_auto_approved(
            multipliers.new_multiplier,
            f64::from_bits(reference_bits),
            daily_auto_approval_bps,
        );

    // Stop refreshing ahead of a scheduled switch so the stored price, computed from the old
    // multiplier, ages out before the new one takes effect rather than staying valid past it.
    // Only the price data changes here, not the price.
    if switch_changes_the_multiplier
        && (1..=TIME_PERIOD_BEFORE_ACTIVATION_TO_SUSPEND_S).contains(&time_to_activation)
    {
        // A suspension approves nothing: the operator resumes with the value the switch brings.
        let recorded = Token2022MultiplierStoredData {
            suspended: true,
            approved_multiplier_bits: None,
            timestamp: Some(switch_timestamp),
        };
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

    // Any other change of the multiplier in effect needs a resume, whatever the mint announced.
    if !effective_is_approved {
        warn!(
            "Scaled multiplier: mint {} prices multiplier {} but {} is approved, suspending its price ({:?})",
            mint_account_info.key(),
            effective_multiplier,
            approved_multiplier,
            stored_data
        );
        // A suspension approves nothing: only a resume records a value again.
        return suspend(Token2022MultiplierStoredData {
            suspended: true,
            approved_multiplier_bits: None,
            timestamp: Some(switch_timestamp),
        });
    }

    // A period starts when the reference rotates onto a new value, and when a threshold is first
    // configured on an entry that was running without one.
    let timestamp = if daily_auto_approval_bps == 0 {
        None
    } else if rotates || period_started.is_none() {
        Some(now)
    } else {
        period_started
    };

    Ok(PriceRefreshOutcome::Updated(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: now,
        generic_data: Token2022MultiplierStoredData {
            suspended: false,
            approved_multiplier_bits: Some(reference_bits),
            timestamp,
        }
        .to_generic_data(),
    }))
}

/// Validate the oracle configuration: the mapped account must be a token-2022 mint whose
/// `ScaledUiAmount` extension parses. The multiplier itself is not checked here — the entry
/// adopts the one in effect on its first refresh, and any change after that suspends it.
pub fn validate_oracle_config(
    mint_account: Option<&AccountInfo>,
    mapping_generic_data: &[u8; 20],
) -> Result<()> {
    let Some(mint_account) = mint_account else {
        warn!("Scaled multiplier: no mint account provided");
        return err!(ScopeError::ExpectedPriceAccount);
    };
    checked_parse_multipliers(mint_account)?;
    let mapping_data = Token2022MultiplierMappingData::from_generic_data(mapping_generic_data)?;
    require_gte!(
        MAX_DAILY_AUTO_APPROVAL_BPS,
        mapping_data.daily_auto_approval_bps,
        ScopeError::AutoApprovalBpsOutOfRange
    );
    Ok(())
}
