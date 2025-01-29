use std::str::FromStr;
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::{pubkey_from_path, signer_from_path};
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use crate::squads::create_transaction;
use crate::utils::{get_shared_args, get_rpc_client, get_account};

const SENTINAL: &str = "sentinal";

pub(crate) async fn init(matches: &ArgMatches) {
    let rpc_client = get_rpc_client(matches);
    let admin = matches.value_of("admin").unwrap();
    let admin_signer = signer_from_path(
        matches,
        admin,
        "admin",
        &mut None,
    ).unwrap();

    let fee_collector_raw = matches.value_of("fee-collector").unwrap();
    let fee_collector: Pubkey = pubkey_from_path(matches, fee_collector_raw, "fee-collector", &mut None).unwrap();

    let init_ix = limitless::client::instructions::admin_init_ix(&fee_collector).unwrap();

    let ms_key_raw = matches.value_of("multisig").unwrap();
    if ms_key_raw == SENTINAL {
        let mut transaction = Transaction::new_with_payer(
            &[init_ix],
            Some(&admin_signer.pubkey()),
        );
        transaction.sign(
            &[admin_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        rpc_client.send_and_confirm_transaction(&transaction).await.unwrap();

        let config_account_key = limitless::client::accounts::derive_limitless_config_pda().unwrap();
        let config_account = get_account::<limitless::state::config::ConfigAccount>(&config_account_key, &rpc_client).await;
        println!("Initialized admin with config {:?}", config_account);
    } else {
        let ms_key: Pubkey = Pubkey::from_str(ms_key_raw).unwrap();
        let mut ms_create_transaction_tx = create_transaction(
            ms_key, admin_signer.pubkey(), vec![init_ix], &rpc_client).await;

        ms_create_transaction_tx.sign(
            &[admin_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        let ms_create_transaction_tx_sig = rpc_client.send_and_confirm_transaction(&ms_create_transaction_tx).await.unwrap();
        println!("Created squads transaction {:?}", ms_create_transaction_tx_sig);
    }
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("init-limitless-admin").
        args(&get_shared_args())
        .arg(
            Arg::new("fee-collector").
                long("fee-collector").
                short('f').
                help("account which will earn fees").
                takes_value(true).
                required(true)
        )
        .arg(
            Arg::new("admin").
                long("admin").
                short('a').
                help("path to admin").
                takes_value(true).
                required(true)
        )
        .arg(
            Arg::new("multisig").
                long("multisig").
                short('m').
                help("multisig address for admin authority").
                takes_value(true).
                default_value(SENTINAL)
        )
}