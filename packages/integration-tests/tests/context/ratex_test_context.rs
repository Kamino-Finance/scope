#![allow(clippy::too_many_arguments)]
#![allow(dead_code)]

use anchor_client::{solana_sdk::signature::Keypair, Program};
use solana_program_test::ProgramTestContext;
use anchor_lang::prelude::Clock;
use std::{cell::RefCell, rc::Rc};
use crate::utilities::helper::{
    create_payer_from_file, get_context, get_sysvar_clock, get_program
};

use super::UserTestContext;

pub struct RateXTestContext {
    pub context: Rc<RefCell<ProgramTestContext>>,
    pub scope_program: Program<Rc<Keypair>>,
    pub admin: Keypair,
    pub users: Vec<UserTestContext>,
}

#[allow(dead_code)]
impl RateXTestContext {
    pub async fn new() -> RateXTestContext {
        let context = get_context().await;

        let scope_program = get_program(scope::id());

        let admin =
            create_payer_from_file(&mut context.borrow_mut(), "tests/fixtures/admin.json").await;

        // Initialize users
        let mut users: Vec<UserTestContext> = vec![];
        for _ in 0..2 {
            let user = UserTestContext::new(context.clone()).await;
            users.push(user);
        }

        RateXTestContext {
            context,
            admin,
            scope_program,
            users,
        }
    }

    pub async fn get_clock(&self) -> Clock {
        let context = &mut self.context.borrow_mut();

        get_sysvar_clock(&mut context.banks_client).await
    }

    pub async fn warp_to_slot(&self) {
        let clock: Clock = self.get_clock().await;

        self.context
            .borrow_mut()
            .warp_to_slot(clock.slot + 1)
            .unwrap();
    }

    pub async fn after(&self, seconds: i64) -> i64 {
        let mut clock: Clock = get_sysvar_clock(&mut self.context.borrow_mut().banks_client).await;

        clock.epoch_start_timestamp += seconds;
        clock.unix_timestamp += seconds;

        self.context.borrow_mut().set_sysvar::<Clock>(&clock);

        clock.unix_timestamp
    }
}

