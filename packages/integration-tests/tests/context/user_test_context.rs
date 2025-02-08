#![allow(clippy::too_many_arguments)]
#![allow(dead_code)]

use anchor_client::solana_sdk::signature::Signer;
use anchor_client::{solana_sdk::signature::Keypair, Program};
use anchor_lang::prelude::{AccountMeta, Pubkey};
use solana_program_test::ProgramTestContext;
use std::{cell::RefCell, rc::Rc};
use crate::utilities::helper::{
    create_user, get_keypair, get_program, HUB_SOL_STAKE_POOL, process_instructions
};
use crate::utilities::instructions::{compose_refresh_price_list_ixs, compose_update_mapping_ixs};

pub struct UserTestContext {
    pub context: Rc<RefCell<ProgramTestContext>>,
    pub scope_program: Program<Rc<Keypair>>,
    pub admin: Keypair,
    pub user: Keypair
}

impl UserTestContext {
    pub async fn new(
        context: Rc<RefCell<ProgramTestContext>>
    ) -> UserTestContext {
        let admin = get_keypair("tests/fixtures/admin.json").await;
        let user = create_user(&mut context.borrow_mut()).await;
        let scope_program = get_program(scope::id());

        UserTestContext {
            context,
            scope_program,
            admin,
            user
        }
    }

    pub async fn refresh_price_list(&self) {
        let context: &mut ProgramTestContext = &mut self.context.borrow_mut();

        let mut remaining_accounts: Vec<AccountMeta> = Vec::new();
        remaining_accounts.append(&mut vec![AccountMeta::new(HUB_SOL_STAKE_POOL, false)]);

        let tokens: Vec<u16> = vec![487];

        let ix = compose_refresh_price_list_ixs(
            &self.scope_program, 
            remaining_accounts, 
            tokens
        );

        process_instructions(context, &self.user, &ix).await;
    }


    pub async fn refresh_ratex_price_list(&self, token: u16, yield_market: &Pubkey, oracle: &Pubkey) {
        let context: &mut ProgramTestContext = &mut self.context.borrow_mut();

        let mut remaining_accounts: Vec<AccountMeta> = Vec::new();
        remaining_accounts.append(&mut vec![AccountMeta::new(*yield_market, false)]);
        remaining_accounts.append(&mut vec![AccountMeta::new(*oracle, false)]);

        let tokens: Vec<u16> = vec![token];

        let ix = compose_refresh_price_list_ixs(
            &self.scope_program, 
            remaining_accounts, 
            tokens
        );

        process_instructions(context, &self.user, &ix).await;
    }    

    pub async fn update_mapping(&self,
        price_info: &Pubkey,
        token: u16,
        price_type: u8,
        twap_enabled: bool,
        twap_source: u16,
        ref_price_index: u16,
        feed_name: String,
        generic_data: [u8; 20]) 
    {
        let context: &mut ProgramTestContext = &mut self.context.borrow_mut();

        let ix = compose_update_mapping_ixs(
            &self.scope_program, 
            &self.admin.pubkey(),
            price_info,
            token,
            price_type,
            twap_enabled,
            twap_source,
            ref_price_index,
            feed_name,
            generic_data
        );

        process_instructions(context, &self.admin, &ix).await;
    }    
}