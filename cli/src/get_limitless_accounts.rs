use std::str::FromStr;
use anchor_lang::AccountDeserialize;
use clap::{Arg, ArgMatches, Command};
use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use limitless::client::accounts::{
    derive_fee_collector_quote_ata,
    derive_limitless_config_pda,
    derive_limitless_event_authority_pda,
    derive_market_account_pda,
    derive_market_intermediate_token_account_pda,
    derive_market_token_account_pda,
    derive_position_account_pda,
};
use limitless::state::config::ConfigAccount;
use limitless::state::liquidity_position::LiquidityPositionAccount;
use limitless::state::market::{MarketAccount, QuoteToken};
use limitless::state::position::PositionAccount;
use utils::raydium::pda::{amm_config_pda, lp_mint_pda, pool_state_pda, token_vault_pda};
use utils::raydium::raydium_cp_swap::accounts::{AmmConfig, PoolState};
use crate::utils::{get_account, get_raw_account, get_rpc_client, get_shared_args, get_token_account, try_get_account, try_get_raw_account};

const SENTINEL_STR: &str = "sentinal";

pub(crate) async fn get_limitless_accounts(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let token_0_mint = Pubkey::from_str(args.value_of("token0").unwrap()).unwrap();
    let token_1_mint = {
        let token_1_mint_raw = args.value_of("token1").unwrap();
        if token_1_mint_raw == SENTINEL_STR {
            spl_token::native_mint::ID
        } else {
            Pubkey::from_str(token_1_mint_raw).unwrap()
        }
    };

    let limitless_config_pda = derive_limitless_config_pda().unwrap();
    let market_pda = derive_market_account_pda(&token_0_mint, &token_1_mint).unwrap();
    let raydium_lp_mint = lp_mint_pda(&token_0_mint, &token_1_mint).unwrap();
    let raydium_token_0_vault = token_vault_pda(&token_0_mint, &token_0_mint, &token_1_mint).unwrap();
    let raydium_token_1_vault = token_vault_pda(&token_1_mint, &token_0_mint, &token_1_mint).unwrap();
    let raydium_pool_state_pda = pool_state_pda(&token_0_mint, &token_1_mint).unwrap();
    let market_token_0_account_pda = derive_market_token_account_pda(&market_pda, &token_0_mint).unwrap();
    let market_token_1_account_pda = derive_market_token_account_pda(&market_pda, &token_1_mint).unwrap();
    let market_lp_token_account_pda = derive_market_token_account_pda(&market_pda, &raydium_lp_mint).unwrap();
    let market_intermediate_token_1_account_pda = derive_market_intermediate_token_account_pda(&market_pda, &token_1_mint).unwrap();
    let limitless_event_authority = derive_limitless_event_authority_pda().unwrap();

    let market = get_account::<MarketAccount>(&market_pda, &rpc_client).await;
    let config_account = get_account::<ConfigAccount>(&limitless_config_pda, &rpc_client).await;
    let fee_collector = config_account.fee_collector;
    let pool_state_raw = get_raw_account(&raydium_pool_state_pda, &rpc_client).await;
    let pool_state = PoolState::try_deserialize(&mut pool_state_raw.data.as_slice()).unwrap();
    let amm_config_pda = amm_config_pda().unwrap();
    let amm_config_raw = get_raw_account(&amm_config_pda, &rpc_client).await;
    let amm_config = AmmConfig::try_deserialize(&mut amm_config_raw.data.as_slice()).unwrap();

    let fee_collector_quote_account_pda = match market.quote_token {
        QuoteToken::Token0 => derive_fee_collector_quote_ata(&fee_collector, &token_0_mint),
        QuoteToken::Token1 => derive_fee_collector_quote_ata(&fee_collector, &token_0_mint),
    };

    println!("-------");
    println!("Limitless States");
    println!("-------");
    println!("limitless_admin address: {}", limitless::admin::ID);
    println!("limitless_event_authority address: {}", limitless_event_authority);
    println!("limitless_config address {}", limitless_config_pda);
    println!("limitless_config account: {:#?}", config_account);
    println!("market address: {}", market_pda);
    println!("market account: {:#?}", market);
    let token_0_balance = get_token_account(&market_token_0_account_pda, &rpc_client).await.amount;
    println!(
        "market token_0 vault: {}, balance: {}",
        market_token_0_account_pda,
        token_0_balance,
    );
    let token_1_balance = get_token_account(&market_token_1_account_pda, &rpc_client).await.amount;
    println!(
        "market token_1 vault: {}, balance: {}",
        market_token_1_account_pda,
        token_1_balance,
    );
    let lp_token_balance = get_token_account(&market_lp_token_account_pda, &rpc_client).await.amount;
    println!(
        "market lp_token vault: {}, balance: {}",
        market_lp_token_account_pda,
        lp_token_balance,
    );
    println!("-------");
    println!("Raydium States");
    println!("-------");
    let token_0_balance = get_token_account(&raydium_token_0_vault, &rpc_client).await.amount;
    println!(
        "raydium token_0 vault: {}, balance: {}, without fees: {}",
        raydium_token_0_vault,
        token_0_balance,
        token_0_balance - pool_state.protocol_fees_token0,
    );
    let token_1_balance = get_token_account(&raydium_token_1_vault, &rpc_client).await.amount;
    println!(
        "raydium token 1 vault: {}, balance: {}, without fees: {}",
        raydium_token_1_vault,
        token_1_balance,
        token_1_balance - pool_state.protocol_fees_token1,
    );
    println!("raydium lp mint: {}", raydium_lp_mint);
    println!("raydium pool state address: {}", raydium_pool_state_pda);
    println!("raydium pool state: {:#?}", pool_state);
    println!("raydium config address: {}", amm_config_pda);
    println!("raydium config: {:#?}", amm_config);
    println!("-------");


    let user = args.value_of("user").map(|v| Pubkey::from_str(v).unwrap());
    let position_id_option = args.value_of("position-id");
    if user.is_some() {
        let user_token_0_account = get_associated_token_address(&user.unwrap(), &token_0_mint);
        let user_token_1_account = get_associated_token_address(&user.unwrap(), &token_1_mint);
        let user_lp_token_account = get_associated_token_address(&user.unwrap(), &raydium_lp_mint);
        println!(
            "user token_0 account: {}, balance: {}",
            user_token_0_account,
            get_token_account(&user_token_0_account, &rpc_client).await.amount,
        );
        match try_get_raw_account(&user_token_1_account, &rpc_client).await {
            Some(a) => {
                println!(
                    "user token_1 account: {}, balance: {}",
                    user_token_1_account,
                    get_token_account(&user_token_1_account, &rpc_client).await.amount,
                )
            },
            None => {
                println!("no token_1 account for user")
            }
        };

        // Check to see if the user has an LP position.
        let user_lp_position_account = limitless::client::accounts::derive_liquidity_position_account_pda(
            &market_pda,
            &user.unwrap(),
        ).unwrap();
        match try_get_account::<LiquidityPositionAccount>(&user_lp_position_account, &rpc_client).await {
            Some(a) => {
                println!("lp position account {:#?}", a)
            },
            None => {
                println!("no lp position account for user")
            }
        };
    }
    if position_id_option.is_some() {
        if user.is_none() {
            panic!("user must be specified when position_id is specified")
        }
        let position_id: uuid::Uuid = position_id_option.unwrap().parse().unwrap();
        let position_pda = derive_position_account_pda(&market_pda, &user.unwrap(), position_id).unwrap();
        let position_account = get_account::<PositionAccount>(&position_pda, &rpc_client).await;
        println!("position address: {}", position_pda);
        println!("position account: {:#?}", position_account);
    }
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("get-limitless-accounts").
        args(&get_shared_args()).
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
            Arg::new("user").
                long("user").
                help("address of user (optional)").
                takes_value(true).
                required(false)
        ).
        arg(
            Arg::new("position-id").
                long("position-id").
                help("Id of position (optional, but user must be specified if this is specified)").
                takes_value(true).
                required(false)
        )
}