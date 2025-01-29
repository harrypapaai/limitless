use std::str::FromStr;
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use crate::utils::{get_rpc_client, get_shared_args};

const GAY_SHIT: &str = "gay_shit";

pub(crate) async fn withdraw_limitless_liquidity(args: &ArgMatches) {
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
    let lp_token_amt: u64 = args.value_of("lp-token-amt").unwrap().parse().unwrap();
    let share_amt: u64 = args.value_of("share-amt").unwrap().parse().unwrap();
    let burn_max: bool = args.value_of("burn-max").unwrap().parse().unwrap();

    let payer_token_0_ata = get_associated_token_address(&payer_signer.pubkey(), &token_0_mint);
    let payer_token_1_ata = get_associated_token_address(&payer_signer.pubkey(), &token_1_mint);

    let ix = limitless::client::instructions::withdraw_liquidity_ix(
        &payer_signer.pubkey(),
        &token_0_mint,
        &token_1_mint,
        &payer_token_0_ata,
        &payer_token_1_ata,
        limitless::instructions::WithdrawLpTokensArgs{
            min_received_lp_tokens: lp_token_amt,
            share_amt,
            burn_max,
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
    let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not withdraw limitless liquidity");
    println!("Signature {}", signature.to_string());
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("withdraw-limitless-liquidity").
        args(&get_shared_args()).
        arg(
            Arg::new("payer").
                long("payer").
                short('p').
                help("path to the payer who is withdrawing the position").
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
            Arg::new("lp-token-amt").
                long("lp-token-amt").
                help("min amount of lp token to withdraw").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("share-amt").
                long("share-amt").
                help("share amount").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("burn-max").
                long("burn-max").
                help("burn max").
                takes_value(true).
                required(true)
        )
}