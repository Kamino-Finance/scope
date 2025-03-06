#![allow(dead_code)]
use std::cell::RefCell;

use anchor_lang::prelude::Pubkey;
use solana_program::pubkey;


use anchor_lang::prelude::Clock;
use solana_program_test::{find_file, read_file, BanksClient, ProgramTest, ProgramTestContext};

use {
    // anchor client must be imported with "async" feature
    anchor_client::{
        solana_sdk::{
            account::Account, commitment_config::CommitmentConfig, instruction::Instruction,
            signature::read_keypair_file, signature::Keypair,
            signature::Signer, system_instruction, sysvar, transaction::Transaction
        },
        Client, Cluster, Program,
    },
    std::rc::Rc,
};

pub const CONFIGURATION: Pubkey = pubkey!("AdTiP7QyjUyv6crF4H8z7fxJKU7Z5eCAGvJN1Y55cXxb");
pub const ORACLE_PRICES: Pubkey = pubkey!("3NJYftD5sjVfxSnUdZ1wVML8f3aC6mp1CXCL6L7TnU8C");
pub const ORACLE_MAPPINGS: Pubkey = pubkey!("Chpu5ZgfWX5ZzVpUx9Xvv4WPM75Xd7zPJNDPsFnCpLpk");
pub const ORACLE_TWAPS: Pubkey = pubkey!("GbpsVomudPRRwmqfTmo3MYQVTikPG6QXxqpzJexA1JRb");

// https://solscan.io/tx/uE2eimdXGUzsnmn4es9uZE2td9Y3TdxBXei5ZaiM4XzeWoQi95NjeN9A9X8iDUXGLVfGnJbrcGzfnr7fZbLBqKs
pub const HUB_SOL_STAKE_POOL: Pubkey = pubkey!("ECRqn7gaNASuvTyC5xfCUjehWZCSowMXstZiM5DNweyB");  // token_id: 487

// https://solscan.io/tx/3VF84ep4GyZimt7x4ebfFdvrASgG2ZTTWuYt1NPZcVx2HGTQWQ23n8urko1MYaeHUsD1yJ3BdUb6ZXf3Y9kGnpY8
pub const JUPITER_LABS_PERPETUALS_MARKETS: Pubkey = pubkey!("5BUwFW4nRbftYTDMbgxykoFWqWHPzahFSNAaaaJtVKsq");

pub const YIELD_MARKET: Pubkey = pubkey!("6Pn9fDvwMSJqnzjSdQg3AV7CESM6hRRHgDMZTpZbQTCs");
pub const ORACLE: Pubkey = pubkey!("9CeH7fy37iHXVtySAekbzFm29mapi8LkErum71vv3po7");

pub fn get_program(pid: Pubkey) -> Program<Rc<Keypair>> {
    let client = Client::new_with_options(
        Cluster::Debug,
        Rc::new(Keypair::new()),
        CommitmentConfig::processed(),
    );
    client.program(pid).unwrap()
}

pub async fn get_context() -> Rc<RefCell<ProgramTestContext>> {
    let mut pt = ProgramTest::new("scope", scope::id(), None);

    // add Configuation and change admin 
    let mut configuration_data = read_file(find_file("AdTiP7QyjUyv6crF4H8z7fxJKU7Z5eCAGvJN1Y55cXxb.bin").unwrap_or_else(|| {
        panic!("Unable to load configuration file");
    }));    

    // let cfg: Configuration = bincode::deserialize(&configuration_data).unwrap();
    // 8+32*6+8*1255
    println!("configuration_data size: {}", configuration_data.len());

    let admin = read_keypair_file("tests/fixtures/admin.json").unwrap();
    configuration_data[8..40].copy_from_slice(&admin.pubkey().to_bytes());

    pt.add_account(
        CONFIGURATION,
        Account {
            lamports: 72_160_000,
            data: configuration_data,
            owner: scope::id(),
            executable: false,
            rent_epoch: 0,
        },
    );   

    pt.add_account_with_file_data(
        ORACLE_PRICES,
        200_700_000,
        scope::id(),
        "3NJYftD5sjVfxSnUdZ1wVML8f3aC6mp1CXCL6L7TnU8C.bin",
    );

    pt.add_account_with_file_data(
        ORACLE_MAPPINGS,
        200_760_000,
        scope::id(),
        "Chpu5ZgfWX5ZzVpUx9Xvv4WPM75Xd7zPJNDPsFnCpLpk.bin",
    );    

    pt.add_account_with_file_data(
        ORACLE_TWAPS,
        2_396_000_000,
        scope::id(),
        "GbpsVomudPRRwmqfTmo3MYQVTikPG6QXxqpzJexA1JRb.bin",
    ); 

    pt.add_account_with_file_data(
        HUB_SOL_STAKE_POOL,
        5_143_000,
        pubkey!("SP12tWFxD9oJsVWNavTTBZvMbA6gkAmxtVgxdqvyvhY"),
        "ECRqn7gaNASuvTyC5xfCUjehWZCSowMXstZiM5DNweyB.bin",
    );    

    pt.add_account_with_file_data(
        JUPITER_LABS_PERPETUALS_MARKETS,
        3_622_000,
        pubkey!("PERPHjGBqRHArX4DySjwM6UJHiR3sWAatqfdBS2qQJu"),
        "5BUwFW4nRbftYTDMbgxykoFWqWHPzahFSNAaaaJtVKsq.bin",
    );      

    pt.add_account_with_file_data(
        YIELD_MARKET,
        1_000_000_000,
        pubkey!("7HeEe9iGQd4N9yBiQ8DXB7ZX7ie1EaUtQQxaL2jz6zQu"),
        "6Pn9fDvwMSJqnzjSdQg3AV7CESM6hRRHgDMZTpZbQTCs.bin",
    );   

    pt.add_account_with_file_data(
        ORACLE,
        1_000_000_000,
        pubkey!("7HeEe9iGQd4N9yBiQ8DXB7ZX7ie1EaUtQQxaL2jz6zQu"),
        "9CeH7fy37iHXVtySAekbzFm29mapi8LkErum71vv3po7.bin",
    );                    

    let context = pt.start_with_context().await;

    Rc::new(RefCell::new(context))
}

pub async fn get_keypair(file_path: &str) -> Keypair {
    read_keypair_file(file_path).unwrap()
}

pub async fn create_payer_from_file(context: &mut ProgramTestContext, file_path: &str) -> Keypair {
    let keypair = read_keypair_file(file_path).unwrap();

    transfer(context, &keypair.pubkey(), 100_000_000_000).await;

    keypair
}

pub async fn create_user(context: &mut ProgramTestContext) -> Keypair {
    let keypair = Keypair::new();

    transfer(context, &keypair.pubkey(), 100_000_000_000).await;

    keypair
}

pub async fn transfer(context: &mut ProgramTestContext, recipient: &Pubkey, amount: u64) {
    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::transfer(
            &context.payer.pubkey(),
            recipient,
            amount,
        )],
        Some(&context.payer.pubkey()),
        &[&context.payer],
        context.banks_client.get_latest_blockhash().await.unwrap(),
    );

    context
        .banks_client
        .process_transaction_with_preflight(transaction)
        .await
        .unwrap();
}

pub async fn process_instructions(
    context: &mut ProgramTestContext,
    admin: &Keypair,
    instructions: &Vec<Instruction>,
) {
    let mut signers: Vec<&Keypair> = vec![];
    signers.push(admin);

    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(&admin.pubkey()),
        &signers,
        context.last_blockhash,
    );

    context
        .banks_client
        .process_transaction_with_commitment(
            transaction,
            anchor_client::solana_sdk::commitment_config::CommitmentLevel::Finalized,
        )
        .await
        .unwrap();

    let clock = get_sysvar_clock(&mut context.banks_client).await;
    context.warp_to_slot(clock.slot + 1).unwrap();

}

pub async fn get_sysvar_clock(banks_client: &mut BanksClient) -> Clock {
    let clock_account = banks_client
        .get_account(sysvar::clock::id())
        .await
        .unwrap()
        .unwrap();

    let clock: Clock = bincode::deserialize(&clock_account.data).unwrap();

    clock
}

pub async fn get_account(banks_client: &mut BanksClient, address: Pubkey) -> Option<Account> {
    banks_client.get_account(address).await.unwrap()
}
