use std::str::FromStr;
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::pubkey::Pubkey;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use uuid::Uuid;
use utils::raydium;
use crate::utils::{get_account, get_rpc_client, get_shared_args, get_token_balance_from_token_account};

const GAY_SHIT: &str = "gay_shit";

pub(crate) async fn open_limitless_position(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();

    let id = {
        let raw_id = args.value_of("id").unwrap();
        if raw_id == GAY_SHIT {
            Uuid::new_v4()
        } else {
            Uuid::parse_str(raw_id).unwrap()
        }
    };
    let token_0_mint = Pubkey::from_str(args.value_of("token0").unwrap()).unwrap();
    let token_1_mint = {
        let token_1_mint_raw = args.value_of("token1").unwrap();
        if token_1_mint_raw == GAY_SHIT {
            spl_token::native_mint::ID
        } else {
            Pubkey::from_str(token_1_mint_raw).unwrap()
        }
    };
    let collateral_amt: u64 = args.value_of("collateral").unwrap().parse().unwrap();
    let is_short: bool = args.is_present("short");
    let duration_blocks: u64 = args.value_of("duration").unwrap().parse().unwrap();
    let max_raydium_fee: u64 = args.value_of("max-raydium-fee").unwrap().parse().unwrap();
    let max_blackwing_fee: u64 = args.value_of("max-blackwing-fee").unwrap().parse().unwrap();
    let rollover_reserve: u64 = args.value_of("rollover-reserve").unwrap().parse().unwrap();
    let worst_price_num: u64 = args.value_of("worst-price-num").unwrap().parse().unwrap();
    let worst_price_den: u64 = args.value_of("worst-price-den").unwrap().parse().unwrap();

    let payer_token_0_ata = get_associated_token_address(&payer_signer.pubkey(), &token_0_mint);
    let payer_token_1_ata = get_associated_token_address(&payer_signer.pubkey(), &token_1_mint);

    let create_token_0_ata_ix = create_associated_token_account_idempotent(
        &payer_signer.pubkey(),
        &payer_signer.pubkey(),
        &token_0_mint,
        &spl_token::ID,
    );
    let create_token_1_ata_ix = create_associated_token_account_idempotent(
        &payer_signer.pubkey(),
        &payer_signer.pubkey(),
        &token_1_mint,
        &spl_token::ID,
    );
    let mut tx = Transaction::new_with_payer(
        &[create_token_0_ata_ix, create_token_1_ata_ix],
        Some(&payer_signer.pubkey()),
    );
    tx.sign(
        &[payer_signer.as_ref()],
        rpc_client.get_latest_blockhash().await.unwrap(),
    );
    let signature = rpc_client.send_and_confirm_transaction(&tx).await
        .expect("could not create token accounts for user");

    let balance_token_1_before = get_token_balance_from_token_account(&payer_token_1_ata, &rpc_client).await;
    println!("Balance token1 before {}", balance_token_1_before);

    let market_account_key = limitless::client::accounts::derive_market_account_pda(
        &token_0_mint,
        &token_1_mint,
    ).unwrap();
    let raydium_lp_mint = raydium::pda::lp_mint_pda(&token_0_mint, &token_1_mint).unwrap();
    let market_lp_token_ata = limitless::client::accounts::derive_market_token_account_pda(
        &market_account_key,
        &raydium_lp_mint,
    ).unwrap();
    println!("Lp token balance {}", get_token_balance_from_token_account(&market_lp_token_ata, &rpc_client).await);

    let cu_ix = ComputeBudgetInstruction::set_compute_unit_limit(500_000);
    let pos_ix = limitless::client::instructions::open_position_ix(
        &payer_signer.pubkey(),
        &token_0_mint,
        &token_1_mint,
        limitless::instructions::OpenPositionArgs{
            id,
            collateral_quote_token_amt: collateral_amt,
            is_short,
            duration_blocks,
            max_raydium_fee_quote_token_amt: max_raydium_fee,
            max_blackwing_fee_quote_token_amt: max_blackwing_fee,
            rollover_reserve_quote_token_amt: rollover_reserve,
            worst_price_num,
            worst_price_den,
        }
    ).unwrap();
    let mut tx = Transaction::new_with_payer(
        &[cu_ix, pos_ix],
        Some(&payer_signer.pubkey()),
    );
    tx.sign(
        &[payer_signer.as_ref()],
        rpc_client.get_latest_blockhash().await.unwrap(),
    );
    let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not open position");

    let balance_token_1_after = get_token_balance_from_token_account(&payer_token_1_ata, &rpc_client).await;

    println!("Balance token1 after {}", balance_token_1_after);
    println!("Signature {}", signature.to_string());

    let market_account_key = limitless::client::accounts::derive_market_account_pda(
        &token_0_mint,
        &token_1_mint,
    ).unwrap();
    let position_account_key = limitless::client::accounts::derive_position_account_pda(
        &market_account_key,
        &payer_signer.pubkey(),
        id,
    ).unwrap();
    let position = get_account::<limitless::state::position::PositionAccount>(
        &position_account_key,
        &rpc_client,
    ).await;
    println!("Position: {:?}", id);
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("open-limitless-position").
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
            Arg::new("id").
                long("id").
                short('i').
                help("id of the position").
                takes_value(true).
                default_value(GAY_SHIT)
        ).
        arg(
            Arg::new("collateral").
                long("collateral").
                short('c').
                help("amount of token 1 collateral to use").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("short").
                long("short").
                short('s').
                help("if the position is a short position")
        ).
        arg(
            Arg::new("duration").
                long("duration").
                short('d').
                help("duration in number of blocks").
                takes_value(true).
                default_value("1000")
        ).
        arg(
            Arg::new("max-raydium-fee").
                long("max-raydium-fee").
                help("max raydium fee to spend in terms of token 1").
                takes_value(true).
                default_value("0")
        ).
        arg(
            Arg::new("max-blackwing-fee").
                long("max-blackwing-fee").
                help("max blackwing fee to spend in terms of token 1").
                takes_value(true).
                default_value("0")
        ).
        arg(
            Arg::new("rollover-reserve").
                long("rollover-reserve").
                help("amount of token 1 to set aside for the rollover reserve").
                takes_value(true).
                default_value("0")
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
}