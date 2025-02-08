#![cfg(test)]

mod context;
mod utilities;

use context::RateXTestContext;
use solana_program_test::*;
use utilities::helper::{ORACLE, YIELD_MARKET};

// Command line to run specific test in single test file
// cd packages/integration-tests
// cargo-test-sbf --test test_scope_ratex -- test_scope_ratex -- --show-output

#[tokio::test]
async fn test_scope_ratex() {
    let rtc = RateXTestContext::new().await;
    let bob = &rtc.users[0]; 

    bob.refresh_price_list().await;

    // Mock update ratex token at 127
    bob.update_mapping(
        &YIELD_MARKET, 
        127, 
        26, // ratex
        false,
        65535, 
        65535, 
        String::from("hubble"),   // feed_name is part of seeds of generating pda of Configuration
        [0;20]
    ).await;    

    bob.refresh_ratex_price_list(127, &YIELD_MARKET, &ORACLE).await;
}