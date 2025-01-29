use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use spl_associated_token_account::get_associated_token_address;
use crate::utils::{get_rpc_client, get_shared_args, get_token_balance_from_token_account, mint_wsol_to_associated_token_account};

pub(crate) async fn mint_wsol(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();

    let payer_wsol_ata = get_associated_token_address(&payer_signer.pubkey(), &spl_token::native_mint::ID);
    let wsol_amt: u64 = args.value_of("amount").unwrap().parse().unwrap();

    let balance_before = get_token_balance_from_token_account(&payer_wsol_ata, &rpc_client).await;

    println!("Balance WSOL before {}", balance_before);

    mint_wsol_to_associated_token_account(&payer_signer, wsol_amt, &rpc_client).await;

    let balance_token_1_after = get_token_balance_from_token_account(&payer_wsol_ata, &rpc_client).await;

    println!("Balance WSOL after {}", balance_token_1_after);
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("mint-wsol").
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
            Arg::new("amount").
                long("amount").
                short('a').
                help("amount of WSOL to mint").
                takes_value(true).
                required(true)
        )
}