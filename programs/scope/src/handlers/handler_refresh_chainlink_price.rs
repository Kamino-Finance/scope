use anchor_lang::prelude::*;
use chainlink_streams_report::report::{
    v10::ReportDataV10, v3::ReportDataV3, v7::ReportDataV7, v8::ReportDataV8, v9::ReportDataV9,
};
use solana_program::program::{get_return_data, invoke};

use crate::{
    oracles::{
        chainlink::{
            self,
            chainlink_streams_itf::{
                self, ACCESS_CONTROLLER_PUBKEY, VERIFIER_CONFIG_PUBKEY, VERIFIER_PROGRAM_ID,
            },
            PriceUpdateResult,
        },
        OracleType,
    },
    states::{OracleMappings, OraclePrices, OracleTwaps},
    utils::price_impl::check_ref_price_difference,
    ScopeError,
};

#[derive(Accounts)]
pub struct RefreshChainlinkPrice<'info> {
    /// The account that signs the transaction.
    pub user: Signer<'info>,

    #[account(mut, has_one = oracle_mappings)]
    pub oracle_prices: AccountLoader<'info, OraclePrices>,

    /// CHECK: Checked above
    pub oracle_mappings: AccountLoader<'info, OracleMappings>,

    #[account(mut, has_one = oracle_prices, has_one = oracle_mappings)]
    pub oracle_twaps: AccountLoader<'info, OracleTwaps>,

    /// The Verifier Account stores the DON's public keys and other verification parameters.
    /// This account must match the PDA derived from the verifier program.
    /// CHECK: The account is validated by the verifier program.
    #[account(address = VERIFIER_CONFIG_PUBKEY)]
    pub verifier_account: AccountInfo<'info>,

    /// The Access Controller Account
    /// CHECK: The account structure is validated by the verifier program.
    #[account(address = ACCESS_CONTROLLER_PUBKEY)]
    pub access_controller: AccountInfo<'info>,
    /// The Config Account is a PDA derived from a signed report
    /// CHECK: The account is validated by the verifier program.
    pub config_account: UncheckedAccount<'info>,
    /// The Verifier Program ID specifies the target Chainlink Data Streams Verifier Program.
    /// CHECK: The program ID is validated by the verifier program.
    #[account(address = VERIFIER_PROGRAM_ID)]
    pub verifier_program_id: AccountInfo<'info>,
}

pub fn refresh_chainlink_price<'info>(
    ctx: Context<'_, '_, '_, 'info, RefreshChainlinkPrice<'info>>,
    token: u16,
    serialized_chainlink_report: Vec<u8>,
) -> Result<()> {
    // 1 - verify the report
    let program_id = ctx.accounts.verifier_program_id.key();
    let verifier_account = ctx.accounts.verifier_account.key();
    let access_controller = ctx.accounts.access_controller.key();
    let user = ctx.accounts.user.key();
    let config_account = ctx.accounts.config_account.key();

    // Create verification instruction
    let chainlink_ix = chainlink_streams_itf::verify(
        &program_id,
        &verifier_account,
        &access_controller,
        &user,
        &config_account,
        serialized_chainlink_report,
    );

    // Invoke the Verifier program
    invoke(
        &chainlink_ix,
        &[
            ctx.accounts.verifier_account.to_account_info(),
            ctx.accounts.access_controller.to_account_info(),
            ctx.accounts.user.to_account_info(),
            ctx.accounts.config_account.to_account_info(),
        ],
    )?;

    let Some((return_program_id, return_data)) = get_return_data() else {
        msg!("No report data found");
        return Err(error!(ScopeError::NoChainlinkReportData));
    };

    require_keys_eq!(
        return_program_id,
        VERIFIER_PROGRAM_ID,
        ScopeError::NoChainlinkReportData
    );

    // 2 - load the report and update the price
    let oracle_mappings = ctx.accounts.oracle_mappings.load()?;
    let mut oracle_twaps = ctx.accounts.oracle_twaps.load_mut()?;
    let mut oracle_prices = ctx.accounts.oracle_prices.load_mut()?;
    let token_idx: usize = token.into();

    let price_update_result = {
        let oracle_mapping = *oracle_mappings
            .price_info_accounts
            .get(token_idx)
            .ok_or(ScopeError::BadTokenNb)?;

        let price_type: OracleType = oracle_mappings.price_types[token_idx]
            .try_into()
            .map_err(|_| ScopeError::BadTokenType)?;
        require!(
            [
                OracleType::Chainlink,
                OracleType::ChainlinkRWA,
                OracleType::ChainlinkNAV,
                OracleType::ChainlinkX,
                OracleType::ChainlinkExchangeRate
            ]
            .contains(&price_type),
            ScopeError::BadTokenType
        );

        let mapping_generic_data = &oracle_mappings.generic[token_idx];

        let dated_price_ref = &mut oracle_prices.prices[token_idx];
        let old_price = *dated_price_ref;
        let clock = Clock::get()?;

        // Decode the verified report data before updating the price
        // Note: for ChainlinkX we might just be updating the current price with the `suspended` flag set to true.
        // In this case we won't be setting a new price (and result will be `SuspendExistingPrice`)
        let price_update_result = match price_type {
            OracleType::Chainlink => {
                let chainlink_report = ReportDataV3::decode(&return_data)
                    .map_err(|_| error!(ScopeError::InvalidChainlinkReportData))?;
                chainlink::update_price_v3(
                    dated_price_ref,
                    oracle_mapping,
                    mapping_generic_data,
                    &clock,
                    &chainlink_report,
                )?
            }
            OracleType::ChainlinkRWA => {
                let chainlink_report = ReportDataV8::decode(&return_data)
                    .map_err(|_| error!(ScopeError::InvalidChainlinkReportData))?;
                chainlink::update_price_v8(
                    dated_price_ref,
                    oracle_mapping,
                    mapping_generic_data,
                    &clock,
                    &chainlink_report,
                )?
            }
            OracleType::ChainlinkNAV => {
                let chainlink_report = ReportDataV9::decode(&return_data)
                    .map_err(|_| error!(ScopeError::InvalidChainlinkReportData))?;
                chainlink::update_price_v9(
                    dated_price_ref,
                    oracle_mapping,
                    &clock,
                    &chainlink_report,
                )?
            }
            OracleType::ChainlinkX => {
                let chainlink_report = ReportDataV10::decode(&return_data)
                    .map_err(|_| error!(ScopeError::InvalidChainlinkReportData))?;
                chainlink::update_price_v10(
                    dated_price_ref,
                    oracle_mapping,
                    mapping_generic_data,
                    &clock,
                    &chainlink_report,
                )?
            }
            OracleType::ChainlinkExchangeRate => {
                let chainlink_report = ReportDataV7::decode(&return_data)
                    .map_err(|_| error!(ScopeError::InvalidChainlinkReportData))?;
                chainlink::update_price_v7(
                    dated_price_ref,
                    oracle_mapping,
                    &clock,
                    &chainlink_report,
                )?
            }
            _ => return Err(error!(ScopeError::BadTokenType)),
        };

        match price_update_result {
            PriceUpdateResult::Updated if oracle_mappings.is_twap_enabled(token_idx) => {
                if let Err(e) = crate::oracles::twap::update_twaps(
                    &mut oracle_twaps,
                    token_idx,
                    dated_price_ref,
                    oracle_mappings.twap_enabled_bitmask[token_idx],
                ) {
                    msg!("Error while updating TWAP of token {token_idx}: {e:?}",);
                }
            }
            _ => {}
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

        price_update_result
    };

    // check that the price is close enough to the ref price if there is a ref price
    match price_update_result {
        PriceUpdateResult::Updated if oracle_mappings.ref_price[token_idx] != u16::MAX => {
            let new_price = oracle_prices.prices[token_idx].price;
            let ref_price =
                oracle_prices.prices[usize::from(oracle_mappings.ref_price[token_idx])].price;
            check_ref_price_difference(new_price, ref_price)?;
        }
        _ => {}
    }

    Ok(())
}
