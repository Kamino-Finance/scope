use anchor_lang::prelude::*;

use crate::{
    oracles::{update_generic_data_must_reset_price, validate_oracle_cfg, OracleType},
    states::{
        Configuration, EmaType, OracleMappings, OraclePrices, OracleTwaps, TokenMetadata,
        TokenMetadatas, TwapEnabledBitmask,
    },
    utils::{list_set_bit_positions, maybe_account, pdas::seeds},
    ScopeError, MAX_ENTRIES, MAX_ENTRIES_U16,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub enum UpdateOracleMappingAndMetadataEntry {
    /// This will remove the entry from the mapping and reset the price entry
    RemoveEntry,
    /// Updating the mapping config requires to provide the mapping account to the instruction.
    ///
    /// This may trigger a reset of the price entry if:
    /// - The price type is changed
    /// - The price info account is changed
    /// - The generic data is changed and the type requires it
    MappingConfig {
        price_type: OracleType,
        generic_data: [u8; 20],
    },
    /// Setting the price type to one of the TWAP types will reset the price entry
    /// The twap will be enabled automatically on the provided `twap_source` entry
    MappingTwapEntry {
        price_type: OracleType,
        twap_source: u16,
    },
    MappingTwapEnabledBitmask(u8),
    MappingRefPrice {
        ref_price_index: Option<u16>,
    },
    MetadataName(String),
    MetadataMaxPriceAgeSlots(u64),
    MetadataGroupIdsBitset(u64),
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateOracleMappingAndMetadataEntriesWithId {
    pub entry_id: u16,
    pub updates: Vec<UpdateOracleMappingAndMetadataEntry>,
}

impl UpdateOracleMappingAndMetadataEntriesWithId {
    pub fn new(entry_id: u16, updates: Vec<UpdateOracleMappingAndMetadataEntry>) -> Self {
        Self { entry_id, updates }
    }
}

/// Handler expects as remaining accounts a `price_info` for each
/// `UpdateOracleMappingAndMetadataEntry::MappingConfig`
/// Entry type when `price_info` is not used are expected to be `crate::ID`
#[derive(Accounts)]
#[instruction(
    feed_name: String,
    updates: Vec<UpdateOracleMappingAndMetadataEntriesWithId>,
)]
pub struct UpdateOracleMappingAndMetadata<'info> {
    pub admin: Signer<'info>,

    #[account(
        seeds = [seeds::CONFIG, feed_name.as_bytes()],
        bump,
        has_one = admin,
        has_one = oracle_mappings,
        has_one = oracle_prices,
        has_one = oracle_twaps,
        has_one = tokens_metadata,
    )]
    pub configuration: AccountLoader<'info, Configuration>,

    #[account(mut)]
    pub oracle_mappings: AccountLoader<'info, OracleMappings>,

    #[account(mut)]
    pub tokens_metadata: AccountLoader<'info, TokenMetadatas>,

    /// Price entry will be reset if the corresponding mapping changes
    #[account(mut, has_one = oracle_mappings)]
    pub oracle_prices: AccountLoader<'info, OraclePrices>,

    /// Twap entry will be reset if the corresponding mapping changes
    #[account(mut, has_one = oracle_mappings, has_one = oracle_prices)]
    pub oracle_twaps: AccountLoader<'info, OracleTwaps>,
}

pub fn process(
    ctx: Context<UpdateOracleMappingAndMetadata>,
    updates: Vec<UpdateOracleMappingAndMetadataEntriesWithId>,
) -> Result<()> {
    // Sanity check, remaining accounts is at most the number of entry modified
    require_gte!(
        updates.len(),
        ctx.remaining_accounts.len(),
        ScopeError::UnexpectedAccount
    );

    // Load accounts
    let mut oracle_mappings = ctx.accounts.oracle_mappings.load_mut()?;
    let mut tokens_metadata = ctx.accounts.tokens_metadata.load_mut()?;
    let metadatas = &mut tokens_metadata.metadatas_array;
    let mut oracle_prices = ctx.accounts.oracle_prices.load_mut()?;
    let mut oracle_twaps = ctx.accounts.oracle_twaps.load_mut()?;
    let clock = Clock::get()?;

    let mut price_info_iter = ctx.remaining_accounts.iter();

    for entry in updates {
        let UpdateOracleMappingAndMetadataEntriesWithId {
            entry_id,
            updates: entry_updates,
        } = entry;
        let entry_id: usize = entry_id.into();
        require_gt!(MAX_ENTRIES, entry_id, ScopeError::BadTokenNb);

        let current_type = oracle_mappings.get_entry_type(entry_id)?;
        let current_mapping_pk = oracle_mappings.get_entry_mapping_pk(entry_id);

        msg!("**********************************");
        let old_name = if oracle_mappings.is_entry_used(entry_id) {
            let name = metadatas[entry_id].get_name();
            msg!(
                "Updating entry {entry_id} - \"{name}\" of type {current_type:?} - pk {current_mapping_pk:?} with {} updates",
                entry_updates.len()
            );
            name.to_string()
        } else {
            msg!(
                "Updating unused entry {entry_id} with {} updates",
                entry_updates.len()
            );

            // Reset all fields to work from a clean state
            oracle_mappings.reset_entry(entry_id);
            oracle_prices.reset_entry(entry_id);
            oracle_twaps.reset_entry(entry_id);
            metadatas[entry_id].reset();

            "<unused>".to_string()
        };

        // To ensure that mapping is updated only once per entry (sanity check).
        let mut mapping_has_been_updated = false;
        let mut mapping_updated_check = move || {
            if mapping_has_been_updated {
                msg!("Mapping config updated twice for the same entry");
                return err!(ScopeError::InvalidUpdateSequenceOrAccounts);
            }
            mapping_has_been_updated = true;
            Ok(())
        };

        msg!("Before updates:");
        msg!("{:?}", oracle_mappings.to_debug_print_entry(entry_id));
        msg!("{:?}", metadatas[entry_id]);

        for update in entry_updates {
            match update {
                UpdateOracleMappingAndMetadataEntry::RemoveEntry => {
                    mapping_updated_check()?;

                    msg!("Removing entry");
                    oracle_mappings.reset_entry(entry_id);
                    oracle_prices.reset_entry(entry_id);
                    oracle_twaps.reset_entry(entry_id);
                    metadatas[entry_id].reset();
                }
                UpdateOracleMappingAndMetadataEntry::MappingConfig {
                    price_type: new_price_type,
                    generic_data: new_generic_data,
                } => {
                    mapping_updated_check()?;

                    if new_price_type.is_twap() {
                        msg!("Use MappingTwapEntry to set TWAP entries");
                        return err!(ScopeError::InvalidUpdateSequenceOrAccounts);
                    }

                    let price_info = price_info_iter
                        .next()
                        .ok_or(ScopeError::MissingPriceAccount)?;
                    let price_info_pk = price_info.key();
                    let price_info_opt = maybe_account(price_info);
                    if price_info_opt.is_none() {
                        msg!("Set oracle mapping to type {new_price_type:?} and generic data {new_generic_data:?} (no account)",);
                    } else {
                        msg!(
                            "Set oracle mapping to type {new_price_type:?} with price info {price_info_pk} and generic data {new_generic_data:?}",
                        );
                    }

                    // Validate the oracle configuration may print more details
                    validate_oracle_cfg(new_price_type, price_info_opt, &new_generic_data, &clock)?;

                    // Reset the twap source (not used for non-TWAP entries)
                    oracle_mappings.twap_source[entry_id] = u16::MAX;

                    let new_mapping_pk = price_info_opt.map(|a| a.key());

                    if current_type != new_price_type
                        || current_mapping_pk != new_mapping_pk
                        || update_generic_data_must_reset_price(new_price_type)
                    {
                        msg!("Resetting price due to mapping config update");
                        oracle_prices.reset_entry(entry_id);
                        oracle_twaps.reset_entry(entry_id);
                    }

                    oracle_mappings.set_entry_mapping(
                        entry_id,
                        new_mapping_pk,
                        new_price_type,
                        new_generic_data,
                    );
                }
                UpdateOracleMappingAndMetadataEntry::MappingTwapEntry {
                    price_type: new_price_type,
                    twap_source,
                } => {
                    mapping_updated_check()?;

                    let target_name =
                        maybe_get_entry_name(&oracle_mappings, metadatas, twap_source.into());

                    if current_type == new_price_type {
                        let current_twap_source = oracle_mappings.get_twap_source(entry_id);
                        msg!("Set TWAP source from {current_twap_source} to {twap_source} - \"{target_name}\"",);
                    } else {
                        msg!("Set oracle mapping to {new_price_type:?} source {twap_source} - \"{target_name}\"",);
                    }

                    oracle_mappings.set_twap_source(entry_id, new_price_type, twap_source)?;

                    let new_ema_type = new_price_type.to_ema_type()?;
                    if !oracle_mappings.is_entry_used(twap_source.into()) {
                        msg!("WARNING: TWAP source entry {twap_source} is not defined",);
                    } else if !oracle_mappings
                        .is_twap_enabled_for_ema_type(twap_source.into(), new_ema_type)
                    {
                        msg!("WARNING: TWAP source entry {twap_source} does not have TWAP type {new_ema_type:?} enabled",);
                    }

                    oracle_prices.reset_entry(entry_id);
                    oracle_twaps.reset_entry(entry_id);
                }
                UpdateOracleMappingAndMetadataEntry::MappingTwapEnabledBitmask(
                    twap_enabled_bitmask,
                ) => {
                    let twap_enabled_bitmask = TwapEnabledBitmask::try_from(twap_enabled_bitmask)?;
                    if twap_enabled_bitmask == oracle_mappings.get_twap_enabled_bitmask(entry_id) {
                        msg!("Twap enabled bitmask is already {twap_enabled_bitmask:?}, skipping",);
                        continue;
                    }
                    for ema_type in [EmaType::Ema1h, EmaType::Ema8h, EmaType::Ema24h] {
                        if !oracle_mappings.is_twap_enabled_for_ema_type(entry_id, ema_type)
                            && twap_enabled_bitmask.is_twap_enabled_for_ema_type(ema_type)
                        {
                            msg!("Enabling {ema_type:?} TWAP",);
                        } else if oracle_mappings.is_twap_enabled_for_ema_type(entry_id, ema_type)
                            && !twap_enabled_bitmask.is_twap_enabled_for_ema_type(ema_type)
                        {
                            msg!("Disabling {ema_type:?} TWAP",);
                        }
                    }

                    oracle_mappings.set_twap_enabled_bitmask(entry_id, twap_enabled_bitmask);
                    oracle_twaps.reset_entry(entry_id);
                }
                UpdateOracleMappingAndMetadataEntry::MappingRefPrice { ref_price_index } => {
                    let old_ref_price = oracle_mappings.get_ref_price(entry_id);
                    let target_name = ref_price_index.as_ref().map(|id| {
                        maybe_get_entry_name(&oracle_mappings, metadatas, usize::from(*id))
                    });
                    let old_target_name = old_ref_price.as_ref().map(|id| {
                        maybe_get_entry_name(&oracle_mappings, metadatas, usize::from(*id))
                    });
                    msg!("Updating ref price from \"{old_target_name:?}\" - {old_ref_price:?} to \"{target_name:?}\" - {ref_price_index:?}",);

                    if let Some(ref_price_index) = ref_price_index {
                        require_gt!(MAX_ENTRIES_U16, ref_price_index, ScopeError::BadTokenNb);
                        if !oracle_mappings.is_entry_used(ref_price_index.into()) {
                            msg!("WARNING: Reference price entry {ref_price_index} is not defined",);
                        }
                    }
                    oracle_mappings.set_ref_price(entry_id, ref_price_index);
                }
                UpdateOracleMappingAndMetadataEntry::MetadataName(new_name) => {
                    msg!("Setting token metadata name from \"{old_name}\" to \"{new_name}\"",);
                    metadatas[entry_id].set_name(&new_name);
                }
                UpdateOracleMappingAndMetadataEntry::MetadataMaxPriceAgeSlots(max_age_slots) => {
                    msg!(
                        "Setting token max age (in slots) from {} to {}",
                        metadatas[entry_id].max_age_price_slots,
                        max_age_slots
                    );
                    metadatas[entry_id].max_age_price_slots = max_age_slots;
                }
                UpdateOracleMappingAndMetadataEntry::MetadataGroupIdsBitset(bitset) => {
                    let old_bitset = metadatas[entry_id].group_ids_bitset;
                    msg!(
                        "Setting token group IDs\n\
                        from: raw {} == binary {:#b} == positions {:?}\n\
                          to: raw {} == binary {:#b} == positions {:?}",
                        old_bitset,
                        old_bitset,
                        list_set_bit_positions(old_bitset),
                        bitset,
                        bitset,
                        list_set_bit_positions(bitset),
                    );
                    metadatas[entry_id].group_ids_bitset = bitset;
                }
            }
        }

        msg!("After updates:");
        msg!("{:?}", oracle_mappings.to_debug_print_entry(entry_id));
        msg!("{:?}", metadatas[entry_id]);
    }

    Ok(())
}

fn maybe_get_entry_name<'a>(
    oracle_mappings: &'_ OracleMappings,
    metadatas: &'a [TokenMetadata],
    entry_id: usize,
) -> &'a str {
    if oracle_mappings.is_entry_used(entry_id) {
        metadatas[entry_id].get_name()
    } else {
        "<unused>"
    }
}
