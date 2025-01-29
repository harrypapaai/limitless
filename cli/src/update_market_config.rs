use std::str::FromStr;

use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;

use crate::squads::create_transaction;
use crate::utils::{get_account, get_rpc_client, get_shared_args};

const SENTINAL: &str = "sentinal";

pub(crate) async fn update_market_config(matches: &ArgMatches) {
    let rpc_client = get_rpc_client(matches);
    let creator = matches.value_of("creator").unwrap();
    let creator_signer = signer_from_path(
        matches, creator, "creator", &mut None).unwrap();

    let token_0_mint = Pubkey::from_str(matches.value_of("token0").unwrap()).unwrap();
    let token_1_mint = {
        let token_1_mint_raw = matches.value_of("token1").unwrap();
        if token_1_mint_raw == SENTINAL {
            spl_token::native_mint::ID
        } else {
            Pubkey::from_str(token_1_mint_raw).unwrap()
        }
    };

    let trading_mode = match matches.value_of("trading-mode") {
        Some(mode) => Some(match mode {
            "disabled" => limitless::state::config::TradingMode::Disabled,
            "enabled" => limitless::state::config::TradingMode::Enabled,
            "pco" => limitless::state::config::TradingMode::PositionCloseOnly,
            _ => { panic!("unknown trading mode: {}", mode) }
        }),
        None => None,
    };
    let max_duration: Option<u64> = matches.value_of("max-duration").map(|s| s.parse().unwrap());
    let min_duration: Option<u64> = matches.value_of("min-duration").map(|s| s.parse().unwrap());
    let base_fee_apr: Option<u64> = matches.value_of("base-fee-apr").map(|s| s.parse().unwrap());
    let min_fee: Option<u64> = matches.value_of("min-fee").map(|s| s.parse().unwrap());

    let ms_key_raw = matches.value_of("multisig").unwrap();
    let user_account_key : Pubkey;
    if ms_key_raw == SENTINAL {
        user_account_key = creator_signer.pubkey()
    } else {
        user_account_key = limitless::admin::ID
    }

    let update_ix = limitless::client::instructions::update_market_config_ix(
        &user_account_key,
        &token_0_mint,
        &token_1_mint,
        limitless::instructions::UpdateMarketConfigArgs {
            trading_mode,
            base_fee_apr,
            min_fee_quote_token: min_fee,
            min_duration_slots: min_duration,
            max_duration_slots: max_duration,
        }
    ).unwrap();

    if ms_key_raw == SENTINAL {
        let mut transaction = Transaction::new_with_payer(
            &[update_ix],
            Some(&creator_signer.pubkey()),
        );
        transaction.sign(
            &[creator_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        rpc_client.send_and_confirm_transaction(&transaction).await.unwrap();
        let market_account_key = limitless::client::accounts::derive_market_account_pda(&token_0_mint, &token_1_mint).unwrap();
        let market_account = get_account::<limitless::state::market::MarketAccount>(&market_account_key, &rpc_client).await;
        println!("Updated market account {:?}", market_account);
    } else {
        let ms_key: Pubkey = Pubkey::from_str(ms_key_raw).unwrap();
        let mut ms_create_transaction_tx = create_transaction(
            ms_key, creator_signer.pubkey(), vec![update_ix], &rpc_client).await;

        ms_create_transaction_tx.sign(
            &[creator_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        let ms_create_transaction_tx_sig = rpc_client.send_and_confirm_transaction(&ms_create_transaction_tx).await.unwrap();
        println!("Created squads transaction {:?}", ms_create_transaction_tx_sig);
    }
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("update-market-config").
        args(&get_shared_args())
        .arg(
            Arg::new("creator").
                long("creator").
                short('c').
                help("account that created the market").
                takes_value(true).
                required(true)
        )
        .arg(
            Arg::new("trading-mode").
                long("trading-mode").
                help("new trading mode of the market").
                takes_value(true).
                required(false)
        )
        .arg(
            Arg::new("max-duration").
                long("max-duration").
                help("new max duration slots of the market").
                takes_value(true).
                required(false)
        )
        .arg(
            Arg::new("min-duration").
                long("min-duration").
                help("new min duration slots of the market").
                takes_value(true).
                required(false)
        )
        .arg(
            Arg::new("base-fee-apr").
                long("base-fee-apr").
                help("new base fee apr").
                takes_value(true).
                required(false)
        )
        .arg(
            Arg::new("min-fee").
                long("min-fee").
                help("new min-fee").
                takes_value(true).
                required(false)
        )
        .arg(
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
                default_value(SENTINAL)
        ).
        arg(
            Arg::new("multisig").
                long("multisig").
                short('m').
                help("multisig address for admin authority").
                takes_value(true).
                default_value(SENTINAL)
        )
}