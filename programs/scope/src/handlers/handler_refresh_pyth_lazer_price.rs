use anchor_lang::{prelude::*, InstructionData};
use pyth_lazer_protocol::{message::SolanaMessage, payload::PayloadData};
use pyth_lazer_solana_contract::{
    self, ID as PYTH_LAZER_PROGRAM_ID, STORAGE_ID as PYTH_LAZER_STORAGE_ID,
    TREASURY_ID as PYTH_LAZER_TREASURY_ID,
};
use solana_program::{
    instruction::Instruction, program::invoke, sysvar::instructions::ID as SYSVAR_INSTRUCTIONS_ID,
};

use crate::{
    oracles::{pyth_lazer, OracleType},
    states::{OracleMappings, OraclePrices, OracleTwaps},
    utils::price_impl::check_ref_price_difference,
    ScopeError,
};

#[derive(Accounts)]
pub struct RefreshPythLazerPrice<'info> {
    /// The account that signs the transaction.
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut, has_one = oracle_mappings)]
    pub oracle_prices: AccountLoader<'info, OraclePrices>,

    /// CHECK: Checked above
    pub oracle_mappings: AccountLoader<'info, OracleMappings>,

    #[account(mut, has_one = oracle_prices, has_one = oracle_mappings)]
    pub oracle_twaps: AccountLoader<'info, OracleTwaps>,

    /// CHECK: This is the Pyth program
    #[account(address = PYTH_LAZER_PROGRAM_ID)]
    pub pyth_program: AccountInfo<'info>,

    /// CHECK: This is the Pyth storage account
    #[account(address = PYTH_LAZER_STORAGE_ID)]
    pub pyth_storage: Account<'info, pyth_lazer_solana_contract::Storage>,

    /// CHECK: This is the Pyth treasury account
    #[account(mut, address = PYTH_LAZER_TREASURY_ID)]
    pub pyth_treasury: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Sysvar fixed address
    #[account(address = SYSVAR_INSTRUCTIONS_ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn refresh_pyth_lazer_price<'info>(
    ctx: Context<'_, '_, '_, 'info, RefreshPythLazerPrice<'info>>,
    tokens: &[u16],
    serialized_pyth_message: Vec<u8>,
    ed25519_instruction_index: u16,
) -> Result<()> {
    // 1 - verify and deserialize the pyth message
    let verify_ix = create_pyth_lazer_verify_ix(
        ctx.accounts,
        &serialized_pyth_message,
        ed25519_instruction_index,
    );

    invoke(
        &verify_ix,
        &[
            ctx.accounts.user.to_account_info(),
            ctx.accounts.pyth_storage.to_account_info(),
            ctx.accounts.pyth_treasury.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.instructions_sysvar.to_account_info(),
        ],
    )
    .map_err(|_| ScopeError::PythLazerVerifyIxFailed)?;

    let solana_message = SolanaMessage::deserialize_slice(&serialized_pyth_message)
        .map_err(|_| ScopeError::PythLazerInvalidMessagePayload)?;
    let payload_data = PayloadData::deserialize_slice_le(&solana_message.payload)
        .map_err(|_| ScopeError::PythLazerInvalidMessagePayload)?;

    // 2 - update the prices
    pyth_lazer::validate_payload_data_for_group(&payload_data, tokens.len())?;
    let oracle_mappings = ctx.accounts.oracle_mappings.load()?;
    let mut oracle_prices = ctx.accounts.oracle_prices.load_mut()?;

    for (i, &token) in tokens.iter().enumerate() {
        let token_idx: usize = token.into();
        let oracle_mapping = *oracle_mappings
            .price_info_accounts
            .get(token_idx)
            .ok_or(ScopeError::BadTokenNb)?;
        require!(
            oracle_mapping == crate::id(),
            ScopeError::PriceAccountNotExpected
        );
        let price_type: OracleType = oracle_mappings.price_types[token_idx]
            .try_into()
            .map_err(|_| ScopeError::BadTokenType)?;
        require!(
            price_type == OracleType::PythLazer,
            ScopeError::BadTokenType
        );

        {
            let dated_price_ref = &mut oracle_prices.prices[token_idx];
            let old_price = *dated_price_ref;
            let mapping_generic_data = &oracle_mappings.generic[token_idx];
            let clock = Clock::get()?;

            match pyth_lazer::update_price(
                dated_price_ref,
                &payload_data,
                i,
                mapping_generic_data,
                &clock,
            ) {
                Ok(()) => (),
                Err(e) => {
                    msg!(
                        "Price skipped as validation failed for token {token_idx}: {:?}",
                        e
                    );
                    continue;
                }
            }

            msg!(
                "tk {}, {:?}: {:?} to {:?} | prev_slot: {:?}, new_slot: {:?}, crt_slot: {:?}",
                token_idx,
                price_type,
                old_price.price.value,
                dated_price_ref.price.value,
                old_price.last_updated_slot,
                dated_price_ref.last_updated_slot,
                clock.slot,
            );

            if oracle_mappings.is_twap_enabled(token_idx) {
                let mut oracle_twaps = ctx.accounts.oracle_twaps.load_mut()?;
                if let Err(e) = crate::oracles::twap::update_twaps(
                    &mut oracle_twaps,
                    token_idx,
                    dated_price_ref,
                    oracle_mappings.twap_enabled_bitmask[token_idx],
                ) {
                    msg!("Error while updating TWAP of token {token_idx}: {e:?}",);
                }
            }
        }

        // check that the price is close enough to the ref price if there is a ref price
        if oracle_mappings.ref_price[token_idx] != u16::MAX {
            let new_price = oracle_prices.prices[token_idx].price;
            let ref_price =
                oracle_prices.prices[usize::from(oracle_mappings.ref_price[token_idx])].price;
            check_ref_price_difference(new_price, ref_price)?;
        }
    }

    Ok(())
}

fn create_pyth_lazer_verify_ix(
    accounts: &RefreshPythLazerPrice,
    serialized_pyth_message: &[u8],
    ed25519_instruction_index: u16,
) -> Instruction {
    Instruction::new_with_bytes(
        PYTH_LAZER_PROGRAM_ID,
        &pyth_lazer_solana_contract::instruction::VerifyMessage {
            message_data: serialized_pyth_message.to_vec(),
            ed25519_instruction_index,
            signature_index: 0,
        }
        .data(),
        vec![
            AccountMeta::new(*accounts.user.key, true),
            AccountMeta::new_readonly(accounts.pyth_storage.key(), false),
            AccountMeta::new(*accounts.pyth_treasury.key, false),
            AccountMeta::new_readonly(*accounts.system_program.key, false),
            AccountMeta::new_readonly(*accounts.instructions_sysvar.key, false),
        ],
    )
}
