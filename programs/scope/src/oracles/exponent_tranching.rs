//! Oracle for Exponent tranching LP prices
//!
//! Prices one side (senior or junior) of an Exponent tranching market: the market's
//! `update_market` instruction syncs the market with its SY rate source and returns the per-LP
//! net asset values as CPI return data.

use anchor_lang::{prelude::*, InstructionData, ToAccountMetas};
use borsh::BorshDeserialize;
use decimal_wad::{common::uint::U192, decimal::Decimal};
use exponent_itf::{
    MarketCpiConfig, MarketReadError, Number, TrancheLpSupplies, UpdateMarketReturnData,
    EVENT_AUTHORITY,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program::{get_return_data, invoke},
};

use crate::{warn, DatedPrice, Price, ScopeError, ScopeResult};

/// `Number` is 1e12 fixed point, [`Decimal`] is 1e18 (WAD).
const NUMBER_SCALE_TO_WAD: u128 =
    decimal_wad::common::WAD as u128 / exponent_itf::NUMBER_DENOMINATOR;

/// `update_market`'s accounts after the market itself, in CPI order.
const FIXED_EXTRA_ACCOUNTS: usize = 5;

/// The serialized `LookupTableMeta` prefix of a lookup table account; the raw 32-byte addresses
/// follow. Mirrors `solana_address_lookup_table_program`'s `LOOKUP_TABLE_META_SIZE`, which the
/// on-chain crate cannot depend on.
pub const LOOKUP_TABLE_META_SIZE: usize = 56;

fn map_market_read_error(market_key: Pubkey, e: MarketReadError) -> ScopeError {
    warn!("Failed to read market {}: {:?}", market_key, e);
    match e {
        MarketReadError::Discriminator => ScopeError::InvalidAccountDiscriminator,
        MarketReadError::Layout => ScopeError::UnableToDeserializeAccount,
    }
}

/// Read the market's CPI config (see [`MarketCpiConfig::from_account_data`]; the full account
/// cannot be deserialized on-chain without overflowing an SBF stack frame).
fn read_market_cpi_config(market: &AccountInfo) -> ScopeResult<MarketCpiConfig> {
    let data = market
        .try_borrow_data()
        .map_err(|_| ScopeError::UnableToDeserializeAccount)?;
    MarketCpiConfig::from_account_data(&data).map_err(|e| map_market_read_error(market.key(), e))
}

/// Read the tranches' LP supplies from the market. Only meaningful AFTER the `update_market` CPI:
/// the sync can materialize pending protocol-fee shares into the totals.
fn read_lp_supplies(market: &AccountInfo) -> ScopeResult<TrancheLpSupplies> {
    let data = market
        .try_borrow_data()
        .map_err(|_| ScopeError::UnableToDeserializeAccount)?;
    TrancheLpSupplies::from_account_data(&data).map_err(|e| map_market_read_error(market.key(), e))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, AnchorDeserialize, AnchorSerialize)]
#[repr(u8)]
pub enum ExponentTrancheSide {
    Senior = 0,
    Junior = 1,
}

#[derive(Debug, AnchorDeserialize, AnchorSerialize)]
pub struct ExponentTranchingData {
    pub tranche_side: ExponentTrancheSide,
}

impl ExponentTranchingData {
    pub fn from_generic_data(mut buff: &[u8]) -> ScopeResult<Self> {
        AnchorDeserialize::deserialize(&mut buff).map_err(|_| {
            warn!("Failed to deserialize ExponentTranchingData");
            ScopeError::InvalidGenericData
        })
    }

    pub fn to_generic_data(&self) -> [u8; 20] {
        let mut buff = [0u8; 20];
        let mut writer = &mut buff[..];
        self.serialize(&mut writer)
            .expect("Failed to serialize ExponentTranchingData");
        buff
    }
}

/// Get the tranche LP price from the Exponent tranching program via CPI.
///
/// # Accounts
/// * `market` - The Exponent tranching market (base account from oracle mapping)
/// * `extra_accounts` - Iterator over extra accounts:
///   0. return_model_storage
///   1. address_lookup_table
///   2. sy_program
///   3. event_authority
///   4. exponent tranching program
///   5. the market's `get_sy_state` accounts (as many as its config lists)
pub fn get_price<'a, 'b>(
    market: &AccountInfo<'a>,
    generic_data: &[u8],
    clock: &Clock,
    extra_accounts: &mut impl Iterator<Item = &'b AccountInfo<'a>>,
) -> ScopeResult<DatedPrice>
where
    'a: 'b,
{
    let config = ExponentTranchingData::from_generic_data(generic_data)?;
    // The reader copies the fields out, so no borrow is held across the CPI.
    let cpi_config = read_market_cpi_config(market)?;
    let sy_contexts = &cpi_config.get_sy_state;

    // Take exactly the accounts this market needs.
    let fixed: Vec<_> = extra_accounts.by_ref().take(FIXED_EXTRA_ACCOUNTS).collect();
    let [return_model, address_lookup_table, sy_program, event_authority, program] =
        fixed.as_slice()
    else {
        return Err(ScopeError::AccountsAndTokenMismatch);
    };
    let sy_accounts: Vec<_> = extra_accounts.take(sy_contexts.len()).collect();
    if sy_accounts.len() != sy_contexts.len() {
        return Err(ScopeError::AccountsAndTokenMismatch);
    }

    // Validate every supplied account against the mapped market account itself — its lookup
    // table, SY program, return-model storage, and (below) the `get_sy_state` list resolved
    // against the market's own table — so no caller-supplied account can substitute them.
    if program.key() != exponent_itf::ID {
        warn!(
            "Unexpected exponent tranching program account: got {}, expected {}",
            program.key(),
            exponent_itf::ID
        );
        return Err(ScopeError::UnexpectedAccount);
    }
    if address_lookup_table.key() != cpi_config.address_lookup_table {
        warn!(
            "Address lookup table mismatch: market expects {}, got {}",
            cpi_config.address_lookup_table,
            address_lookup_table.key()
        );
        return Err(ScopeError::UnexpectedAccount);
    }
    if sy_program.key() != cpi_config.sy_program {
        warn!(
            "SY program mismatch: market expects {}, got {}",
            cpi_config.sy_program,
            sy_program.key()
        );
        return Err(ScopeError::UnexpectedAccount);
    }
    if return_model.key() != cpi_config.return_model_storage {
        warn!(
            "Return model storage mismatch: market expects {}, got {}",
            cpi_config.return_model_storage,
            return_model.key()
        );
        return Err(ScopeError::UnexpectedAccount);
    }
    if event_authority.key() != EVENT_AUTHORITY {
        warn!(
            "Event authority mismatch: expected {}, got {}",
            EVENT_AUTHORITY,
            event_authority.key()
        );
        return Err(ScopeError::UnexpectedAccount);
    }

    let mut metas = exponent_itf::accounts::UpdateMarket {
        market: market.key(),
        return_model_storage: return_model.key(),
        address_lookup_table: address_lookup_table.key(),
        sy_program: sy_program.key(),
        event_authority: event_authority.key(),
        program: program.key(),
    }
    .to_account_metas(None);
    metas.reserve(sy_accounts.len());
    {
        // The lookup table's addresses start after its serialized meta; resolve each configured
        // index and check the supplied account, taking the meta flags from the market's config
        // (scope never signs a CPI account). The borrow ends before `invoke`.
        let alt_data = address_lookup_table
            .try_borrow_data()
            .map_err(|_| ScopeError::UnableToDeserializeAccount)?;
        for (context, account) in sy_contexts.iter().zip(sy_accounts.iter()) {
            let expected = alt_address(&alt_data, context.alt_index)?;
            if account.key() != expected {
                warn!(
                    "SY account mismatch at lookup table index {}: market expects {}, got {}",
                    context.alt_index,
                    expected,
                    account.key()
                );
                return Err(ScopeError::UnexpectedAccount);
            }
            if context.is_signer {
                warn!(
                    "get_sy_state account {} requires a signer; scope cannot sign",
                    expected
                );
                return Err(ScopeError::UnexpectedAccount);
            }
            metas.push(AccountMeta {
                pubkey: account.key(),
                is_signer: false,
                is_writable: context.is_writable,
            });
        }
    }

    let mut account_infos = Vec::with_capacity(2 + FIXED_EXTRA_ACCOUNTS + sy_accounts.len());
    account_infos.push(program.to_account_info());
    account_infos.push(market.to_account_info());
    account_infos.extend(fixed.iter().map(|account| account.to_account_info()));
    account_infos.extend(sy_accounts.iter().map(|account| account.to_account_info()));

    invoke(
        &Instruction {
            program_id: exponent_itf::ID,
            accounts: metas,
            data: exponent_itf::instruction::UpdateMarket {}.data(),
        },
        &account_infos,
    )
    .expect("update_market invoke returned Err; an Exponent revert aborts the tx instead, so this is a pre-syscall bug (e.g. a held account borrow)");

    let (return_program, return_bytes) = get_return_data().ok_or_else(|| {
        warn!("No return data from exponent update_market");
        ScopeError::ExponentTranchingCPIError
    })?;
    if return_program != exponent_itf::ID {
        warn!(
            "Return data from unexpected program: {} (expected {})",
            return_program,
            exponent_itf::ID
        );
        return Err(ScopeError::ExponentTranchingCPIError);
    }
    let return_data = UpdateMarketReturnData::try_from_slice(&return_bytes).map_err(|e| {
        warn!(
            "Failed to deserialize exponent update_market return data: {:?}",
            e
        );
        ScopeError::ExponentTranchingCPIError
    })?;
    if return_data.market != market.key() {
        warn!(
            "update_market returned data for market {}, expected {}",
            return_data.market,
            market.key()
        );
        return Err(ScopeError::ExponentTranchingCPIError);
    }

    let supplies = read_lp_supplies(market)?;
    let (lp_price, lp_supply) = match config.tranche_side {
        ExponentTrancheSide::Senior => (return_data.senior_lp_price_net_asset, supplies.senior),
        ExponentTrancheSide::Junior => (return_data.junior_lp_price_net_asset, supplies.junior),
    };
    // A tranche with zero LP supply (never seeded, or fully withdrawn) has nothing to price: keep
    // the entry invalid rather than publish a fresh value. A wiped tranche (supply > 0, zero NAV)
    // still has an LP price, the virtual-share value deposits mint at, and publishes it.
    if lp_supply == 0 {
        warn!(
            "Market {} {:?} tranche has zero LP supply (never seeded, or fully withdrawn)",
            market.key(),
            config.tranche_side
        );
        return Err(ScopeError::PriceNotValid);
    }
    let price = to_scope_price(lp_price)?;

    // `update_market` stamps the market's `last_updated_slot` with the current slot during the
    // CPI (confirmed by the tranching authors), so the sync moment is this transaction's clock:
    // slot and timestamp are taken from it as a consistent pair.
    Ok(DatedPrice {
        price,
        last_updated_slot: clock.slot,
        unix_timestamp: u64::try_from(clock.unix_timestamp)
            .map_err(|_| ScopeError::BadTimestamp)?,
        ..Default::default()
    })
}

/// The address at `index` of a lookup table account's data.
fn alt_address(alt_data: &[u8], index: u8) -> ScopeResult<Pubkey> {
    let start = LOOKUP_TABLE_META_SIZE + usize::from(index) * 32;
    alt_data
        .get(start..start + 32)
        .map(|bytes| Pubkey::new_from_array(bytes.try_into().unwrap()))
        .ok_or_else(|| {
            warn!("Lookup table has no address at index {}", index);
            ScopeError::UnexpectedAccount
        })
}

/// A tranche's LP price as the market reports it: its per-LP net asset value. For a wiped tranche
/// (zero effective NAV) that is the virtual-share value deposits mint at, which can round to zero
/// at real supplies; the type is in `allows_zero_price`. Callers gate the zero-LP-supply case
/// first — a supply-less tranche must NOT price.
fn to_scope_price(lp_price: Number) -> ScopeResult<Price> {
    let scaled_price = U192::from(lp_price.raw_u128().ok_or(ScopeError::MathOverflow)?)
        .checked_mul(U192::from(NUMBER_SCALE_TO_WAD))
        .ok_or(ScopeError::MathOverflow)?;
    Decimal::from_scaled_val(scaled_price).try_into()
}

pub fn validate_mapping_cfg(mapping: Option<&AccountInfo>, generic_data: &[u8]) -> ScopeResult<()> {
    let market = mapping.ok_or(ScopeError::MissingPriceAccount)?;
    if market.owner != &exponent_itf::ID {
        warn!(
            "Market owner is {} but expected {}",
            market.owner,
            exponent_itf::ID
        );
        return Err(ScopeError::WrongAccountOwner);
    }
    // The reader checks the discriminator and the layout down to the CPI account list.
    read_market_cpi_config(market)?;
    ExponentTranchingData::from_generic_data(generic_data)?;
    Ok(())
}
