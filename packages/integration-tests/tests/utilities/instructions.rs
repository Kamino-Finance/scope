#![allow(clippy::too_many_arguments)]
#![allow(dead_code)]

use std::rc::Rc;

use anchor_client::{
    solana_sdk::{
        compute_budget, instruction::Instruction, sysvar::{self}
    },
    Program,
};
use anchor_lang::prelude::{AccountMeta, Pubkey};
use anchor_client::solana_sdk::signature::Keypair;

use scope::accounts::{RefreshList, UpdateOracleMapping};

use crate::utilities::helper::{ORACLE_PRICES, ORACLE_MAPPINGS, ORACLE_TWAPS, CONFIGURATION};

pub fn compose_refresh_price_list_ixs(
    program: &Program<Rc<Keypair>>,
    remaining_accounts: Vec<AccountMeta>,
    tokens: Vec<u16>
) -> Vec<Instruction> {
    program
        .request()
        .instruction(compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1400000))
        .accounts(RefreshList {
            oracle_prices: ORACLE_PRICES,
            oracle_mappings: ORACLE_MAPPINGS,
            oracle_twaps: ORACLE_TWAPS,
            instruction_sysvar_account_info: sysvar::instructions::ID
        })
        .accounts(remaining_accounts)
        .args(scope::instruction::RefreshPriceList {
            tokens
        })
        .instructions()
        .unwrap()    
}

pub fn compose_update_mapping_ixs(
    program: &Program<Rc<Keypair>>,
    admin: &Pubkey,
    price_info: &Pubkey,
    token: u16,
    price_type: u8,
    twap_enabled: bool,
    twap_source: u16,
    ref_price_index: u16,
    feed_name: String,
    generic_data: [u8; 20]
) -> Vec<Instruction> {
    program
        .request()
        .instruction(compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1400000))
        .accounts(UpdateOracleMapping {
            admin: *admin,
            configuration: CONFIGURATION,
            oracle_mappings: ORACLE_MAPPINGS,
            price_info: Some(*price_info)
        })
        .args(scope::instruction::UpdateMapping {
            token, price_type, twap_enabled, twap_source, ref_price_index, feed_name, generic_data
        })
        .instructions()
        .unwrap()    
}