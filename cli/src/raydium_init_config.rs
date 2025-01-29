use std::str::FromStr;
use anchor_lang::{InstructionData, ToAccountMetas};
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use crate::squads::create_transaction;
use crate::utils::{get_shared_args, get_rpc_client};

const SENTINAL: &str = "sentinal";

pub(crate) async fn init(matches: &ArgMatches) {
    let rpc_client = get_rpc_client(matches);

    // who will be paying for this transaction
    let payer = matches.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        matches, payer, "payer", &mut None).unwrap();

    // address of the raydium admin
    let admin = Pubkey::from_str(matches.value_of("admin").unwrap()).unwrap();

    // config params
    let trade_fee_rate: u64 = matches.value_of("trade-fee-rate").unwrap().parse().unwrap();
    let protocol_fee_rate: u64 = matches.value_of("protocol-fee-rate").unwrap().parse().unwrap();
    let fund_fee_rate: u64 = matches.value_of("fund-fee-rate").unwrap().parse().unwrap();
    let create_pool_fee: u64 = matches.value_of("create-pool-fee").unwrap().parse().unwrap();

    let amm_config_account = utils::raydium::pda::amm_config_pda().unwrap();
    let create_config_accounts = utils::raydium::raydium_cp_swap::client::accounts::CreateAmmConfig {
        owner: admin,
        amm_config: amm_config_account,
        system_program: solana_program::system_program::id(),
    };
    let create_config_args = utils::raydium::raydium_cp_swap::client::args::CreateAmmConfig {
        index: utils::raydium::INDEX,
        trade_fee_rate,
        protocol_fee_rate,
        fund_fee_rate,
        create_pool_fee
    };
    let create_config_args_data = create_config_args.data();
    let create_config_ix = Instruction::new_with_bytes(
        utils::raydium::ID,
        create_config_args_data.as_slice(),
        create_config_accounts.to_account_metas(None),
    );

    let ms_key_raw = matches.value_of("multisig").unwrap();
    if ms_key_raw == SENTINAL {
        let mut tx = Transaction::new_with_payer(
            &[create_config_ix],
            Some(&payer_signer.pubkey()),
        );
        tx.sign(
            &[payer_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not create config");
        println!("created amm config, signature: {}", signature);
    } else {
        let ms_key: Pubkey = Pubkey::from_str(ms_key_raw).unwrap();
        let mut ms_create_transaction_tx = create_transaction(
            ms_key, payer_signer.pubkey(), vec![create_config_ix], &rpc_client).await;

        ms_create_transaction_tx.sign(
            &[payer_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        let ms_create_transaction_tx_sig = rpc_client.send_and_confirm_transaction(&ms_create_transaction_tx).await.unwrap();
        println!("Created squads transaction {:?}", ms_create_transaction_tx_sig);
    }
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("raydium-init-config").
        args(&get_shared_args()).
        arg(
            Arg::new("payer").
                long("payer").
                short('p').
                help("payer that will pay for the transaction").
                takes_value(true).
                required(true)
        ).arg(
            Arg::new("admin").
                long("admin").
                short('a').
                help("raydium admin").
                takes_value(true).
                required(true)
        ).arg(
            Arg::new("multisig").
                long("multisig").
                short('m').
                help("multisig address for admin authority").
                takes_value(true).
                default_value(SENTINAL)
        ).
        arg(
            Arg::new("trade-fee-rate").
                long("trade-fee-rate").
                help("trade fee rate").
                takes_value(true).
                default_value("0")
        ).
        arg(
            Arg::new("protocol-fee-rate").
                long("protocol-fee-rate").
                help("protocol fee rate").
                takes_value(true).
                default_value("0")
        ).
        arg(
            Arg::new("fund-fee-rate").
                long("fund-fee-rate").
                help("fund fee rate").
                takes_value(true).
                default_value("0")
        ).
        arg(
            Arg::new("create-pool-fee").
                long("create-pool-fee").
                help("create pool fee").
                takes_value(true).
                default_value("0")
        )
}