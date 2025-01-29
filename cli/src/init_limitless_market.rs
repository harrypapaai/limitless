use std::str::FromStr;
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::{pubkey_from_path, signer_from_path};
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use limitless::instructions::InitMarketArgs;
use limitless::state::config::TradingMode;
use limitless::state::market::QuoteToken;
use crate::utils::{get_rpc_client, get_shared_args, validate_fee_collector};

const GAY_SHIT: &str = "gay_shit";

pub(crate) async fn init(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();

    let token_0_mint = Pubkey::from_str(args.value_of("token0").unwrap()).unwrap();
    let token_1_mint = {
        let token_1_mint_raw = args.value_of("token1").unwrap();
        if token_1_mint_raw == GAY_SHIT {
            spl_token::native_mint::ID
        } else {
            Pubkey::from_str(token_1_mint_raw).unwrap()
        }
    };

    let fee_collector_raw = args.value_of("fee-collector").unwrap();
    let fee_collector: Pubkey = pubkey_from_path(args, fee_collector_raw, "fee-collector", &mut None).unwrap();
    validate_fee_collector(&fee_collector, &rpc_client).await;

    let ix = limitless::client::instructions::init_market_ix(
        &payer_signer.pubkey(),
        &token_0_mint,
        &token_1_mint,
        &fee_collector,
        InitMarketArgs{
            trading_mode: TradingMode::Enabled,
            quote_token: QuoteToken::Token1,
            base_fee_apr: 100000,
            min_fee_quote_token: 700000,
            min_duration_slots: 9000,
            max_duration_slots: 302400000,
        }
    ).unwrap();
    let mut tx = Transaction::new_with_payer(
        &[ix],
        Some(&payer_signer.pubkey()),
    );
    tx.sign(
        &[payer_signer.as_ref()],
        rpc_client.get_latest_blockhash().await.unwrap(),
    );
    let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not create limitless market");
    println!("Signature {}", signature.to_string());
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("init-limitless-market").
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
                default_value(GAY_SHIT)
        ).
        arg(
            Arg::new("fee-collector").
                long("fee-collector").
                short('f').
                help("wallet where fees are sent to").
                required(true).
                takes_value(true)
        )
}