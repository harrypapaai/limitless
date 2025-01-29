use std::str::FromStr;
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::{pubkey_from_path, signer_from_path};
use solana_program::pubkey::Pubkey;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use uuid::Uuid;
use crate::utils::{get_account, get_raw_account, get_rpc_client, get_shared_args, get_token_balance_from_token_account, validate_fee_collector};

const SENTINEL_STR: &str = "gay_shit";

pub(crate) async fn close_limitless_position(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();

    let fee_collector_raw = args.value_of("fee-collector").unwrap();
    let fee_collector: Pubkey = pubkey_from_path(args, fee_collector_raw, "fee-collector", &mut None).unwrap();
    validate_fee_collector(&fee_collector, &rpc_client).await;

    let id = Uuid::parse_str(args.value_of("id").unwrap()).unwrap();
    let token_0_mint = Pubkey::from_str(args.value_of("token0").unwrap()).unwrap();
    let token_1_mint = {
        let token_1_mint_raw = args.value_of("token1").unwrap();
        if token_1_mint_raw == SENTINEL_STR {
            spl_token::native_mint::ID
        } else {
            Pubkey::from_str(token_1_mint_raw).unwrap()
        }
    };
    let worst_price_num: u64 = args.value_of("worst-price-num").unwrap().parse().unwrap();
    let worst_price_den: u64 = args.value_of("worst-price-den").unwrap().parse().unwrap();
    let as_external = args.is_present("as_external");

    let payer_token_0_ata = get_associated_token_address(&payer_signer.pubkey(), &token_0_mint);
    let payer_token_1_ata = get_associated_token_address(&payer_signer.pubkey(), &token_1_mint);

    let balance_token_1_before = get_token_balance_from_token_account(&payer_token_1_ata, &rpc_client).await;
    let balance_sol_before = get_raw_account(&payer_signer.pubkey(), &rpc_client).await.lamports;
    println!("Balance sol before {}", balance_sol_before);
    println!("Balance token1 before {}", balance_token_1_before);

    let market_account_key = limitless::client::accounts::derive_market_account_pda(
        &token_0_mint,
        &token_1_mint,
    ).unwrap();
    let config_account_key = limitless::client::accounts::derive_limitless_config_pda().unwrap();

    let cu_ix = ComputeBudgetInstruction::set_compute_unit_limit(500_000);

    let config_account = get_account::<limitless::state::config::ConfigAccount>(&config_account_key, &rpc_client).await;
    let market_account = get_account::<limitless::state::market::MarketAccount>(&market_account_key, &rpc_client).await;

    let close_ix = limitless::client::instructions::close_position_ix(
        &payer_signer.pubkey(),
        &payer_signer.pubkey(),
        !as_external,
        &token_0_mint,
        &token_1_mint,
        &config_account.fee_collector,
        market_account.quote_token,
        limitless::instructions::ClosePositionArgs{
            id,
            worst_price_num,
            worst_price_den,
        }
    ).unwrap();
    let mut tx = Transaction::new_with_payer(
        &[cu_ix, close_ix],
        Some(&payer_signer.pubkey()),
    );
    tx.sign(
        &[payer_signer.as_ref()],
        rpc_client.get_latest_blockhash().await.unwrap(),
    );
    let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not close position");

    let balance_token_1_after = get_token_balance_from_token_account(&payer_token_1_ata, &rpc_client).await;

    println!("Balance token1 after {}", balance_token_1_after);
    let balance_sol_after = get_raw_account(&payer_signer.pubkey(), &rpc_client).await.lamports;
    println!("Balance sol after {}", balance_sol_after);
    println!("Signature {}", signature.to_string());
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("close-limitless-position").
        args(&get_shared_args()).
        arg(
            Arg::new("payer").
                long("payer").
                short('p').
                help("path to the payer who is opening the position").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("token0").
                long("token0").
                help("token0 mint").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("token1").
                long("token1").
                help("token1 mint").
                takes_value(true).
                default_value(SENTINEL_STR)
        ).
        arg(
            Arg::new("id").
                long("id").
                short('i').
                help("id of the position").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("as_external").
                long("as_external").
                help("if the position is a short position").
                default_value("false")
        ).
        arg(
            Arg::new("worst-price-num").
                long("worst-price-num").
                help("numerator for worst price").
                takes_value(true).
                default_value("0")
        ).
        arg(
            Arg::new("worst-price-den").
                long("worst-price-den").
                help("denominator for worst price").
                takes_value(true).
                default_value("1")
        )
        .arg(
            Arg::new("fee-collector").
                long("fee-collector").
                short('f').
                help("wallet where fees are sent to").
                required(true).
                takes_value(true)
        )
}