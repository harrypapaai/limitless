use std::fs;
use std::fs::OpenOptions;
use std::io::{Write};
use borsh::BorshDeserialize;
use clap::{Arg, ArgMatches};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_program::sysvar::SysvarId;
use solana_sdk::account::Account;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use spl_token::instruction::sync_native;
use spl_token::state::Account as TokenAccount;

pub(crate) fn set_state(name: &str, val: String) {
    fs::create_dir_all("./.cli_state").unwrap();
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(format!("./.cli_state/{}", name))
        .expect("Could not open file");
    file.write_all(val.as_bytes()).unwrap();
}

pub(crate) async fn get_rent(client: &RpcClient) -> Rent {
    let rent_account = client.get_account(&Rent::id()).await.unwrap();
    bincode::deserialize(&rent_account.data).unwrap()
}

pub(crate) fn get_rpc_client(matches: &ArgMatches) -> RpcClient {
    let rpc_url = matches.value_of("rpc-url").unwrap();
    RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::processed())
}

pub(crate) fn get_shared_args<'a>() -> [Arg<'a>; 1] {
    [
        Arg::with_name("rpc-url").
            long("rpc-url").
            short('r').
            help("RPC URL").
            takes_value(true).
            default_value("http://localhost:8899"),
    ]
}

pub(crate) async fn transfer_lamports(payer: &Box<dyn Signer>, amount: u64, destination: &Pubkey, rpc_client: &RpcClient) {
    let transfer_ix = solana_sdk::system_instruction::transfer(
        &payer.pubkey(),
        &destination,
        amount,
    );

    let blockhash = rpc_client.get_latest_blockhash().await.unwrap();

    let mut tx = Transaction::new_with_payer(
        &[transfer_ix],
        Some(&payer.pubkey()),
    );
    tx.sign(
        &[payer.as_ref()],
        blockhash
    );
    rpc_client.send_and_confirm_transaction(&tx).await.expect("could not transfer sol");
}

pub(crate) async fn transfer_spl_token(payer: &Box<dyn Signer>, token_mint: &Pubkey, amount: u64, destination: &Pubkey, rpc_client: &RpcClient) {
    let payer_token_ata = get_associated_token_address(&payer.pubkey(), token_mint);
    let destination_token_ata = get_associated_token_address(destination, token_mint);
    let transfer_ix = spl_token::instruction::transfer(
        &spl_token::ID,
        &payer_token_ata,
        &destination_token_ata,
        &payer.pubkey(),
        &[&payer.pubkey()],
        amount,
    ).unwrap();

    let create_destination_token_ata_ix = create_associated_token_account_idempotent(
        &payer.pubkey(),
        &destination,
        &token_mint,
        &spl_token::ID,
    );

    let blockhash = rpc_client.get_latest_blockhash().await.unwrap();

    let mut tx = Transaction::new_with_payer(
        &[create_destination_token_ata_ix, transfer_ix],
        Some(&payer.pubkey()),
    );
    tx.sign(
        &[payer.as_ref()],
        blockhash
    );
    rpc_client.send_and_confirm_transaction(&tx).await.expect("could not transfer spl token");
}

pub(crate) async fn mint_wsol_to_associated_token_account(payer: &Box<dyn Signer>, amount: u64, rpc_client: &RpcClient) {
    let payer_wsol_token_account_pda = get_associated_token_address(&payer.pubkey(), &spl_token::native_mint::ID);
    let create_source_wsol_token_account_ix = create_associated_token_account_idempotent(
        &payer.pubkey(),
        &payer.pubkey(),
        &spl_token::native_mint::ID,
        &spl_token::ID,
    );
    let wrap_sol_ix = solana_sdk::system_instruction::transfer(
        &payer.pubkey(),
        &payer_wsol_token_account_pda,
        amount,
    );
    let sync_native_ix = sync_native(
        &spl_token::ID,
        &payer_wsol_token_account_pda,
    ).unwrap();

    let blockhash = rpc_client.get_latest_blockhash().await.unwrap();

    let mut wrap_sol_tx = Transaction::new_with_payer(
        &[create_source_wsol_token_account_ix, wrap_sol_ix, sync_native_ix],
        Some(&payer.pubkey()),
    );
    wrap_sol_tx.sign(
        &[payer.as_ref()],
        blockhash
    );
    rpc_client.send_and_confirm_transaction(&wrap_sol_tx).await.expect("could not wrap sol");
}

pub(crate) async fn create_token_account(payer: &Box<dyn Signer>, owner: &Pubkey, mint: &Pubkey, rpc_client: &RpcClient) {
    let ix = create_associated_token_account_idempotent(
        &payer.pubkey(),
        owner,
        mint,
        &spl_token::ID,
    );

    let blockhash = rpc_client.get_latest_blockhash().await.unwrap();

    let mut wrap_sol_tx = Transaction::new_with_payer(
        &[ix],
        Some(&payer.pubkey()),
    );
    wrap_sol_tx.sign(
        &[payer.as_ref()],
        blockhash
    );
    rpc_client.send_and_confirm_transaction(&wrap_sol_tx).await.expect("could not create token account");
}

pub async fn get_token_balance_from_token_account(ata: &Pubkey, rpc_client: &RpcClient) -> u64 {
    match rpc_client.get_account(ata).await {
        Ok(acc) => {
            let account = TokenAccount::unpack(&acc.data).unwrap();
            account.amount
        }
        Err(_) => 0,
    }
}
pub async fn get_token_account(ata: &Pubkey, rpc_client: &RpcClient) -> spl_token::state::Account {
    let acc = rpc_client.get_account(ata).await.unwrap();
    TokenAccount::unpack(&acc.data).unwrap()
}

pub async fn get_account<T: BorshDeserialize>(key: &Pubkey, rpc_client: &RpcClient) -> T {
    try_get_account(key, rpc_client).await.unwrap()
}

pub async fn try_get_account<T: BorshDeserialize>(key: &Pubkey, rpc_client: &RpcClient) -> Option<T> {
    match rpc_client.get_account(key).await {
        Ok(acc) => {
            T::try_from_slice(&acc.data).map_or_else(|_| None, |a| Some(a))
        },
        Err(e) => {
            println!("error getting account {:#?}", e);
            None
        },
    }
}

pub async fn get_raw_account(key: &Pubkey, rpc_client: &RpcClient) -> Account {
    try_get_raw_account(key, rpc_client).await.unwrap()
}

pub async fn try_get_raw_account(key: &Pubkey, rpc_client: &RpcClient) -> Option<Account> {
    rpc_client.get_account(key).await.map_or_else(|_| None, |a| Some(a))
}

pub async fn validate_fee_collector(fee_collector: &Pubkey, rpc_client: &RpcClient) {
    let config_account_key = limitless::client::accounts::derive_limitless_config_pda().unwrap();
    let config_account = get_account::<limitless::state::config::ConfigAccount>(&config_account_key, &rpc_client).await;
    if !fee_collector.eq(&config_account.fee_collector) {
        panic!(
            "Fee collector provided {} does not match config fee collector {}",
            fee_collector.to_string(),
            config_account.fee_collector.to_string(),
        );
    }
}
