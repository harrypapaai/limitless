use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use crate::utils::{get_rpc_client, get_shared_args, get_token_balance_from_token_account, transfer_spl_token};

pub(crate) async fn transfer_spl(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();
    println!("payer {}", payer_signer.pubkey());
    let token_mint: Pubkey = args.value_of("token").unwrap().parse().unwrap();
    let payer_token_ata = get_associated_token_address(&payer_signer.pubkey(), &token_mint);
    let destination: Pubkey = args.value_of("destination").unwrap().parse().unwrap();
    let destination_token_ata = get_associated_token_address(&destination, &token_mint);
    let amt: u64 = args.value_of("amount").unwrap().parse().unwrap();

    let payer_balance_before = get_token_balance_from_token_account(&payer_token_ata, &rpc_client).await;
    println!("Payer token balance before {}", payer_balance_before);
    let destination_balance_before = get_token_balance_from_token_account(&destination_token_ata, &rpc_client).await;
    println!("Destination token balance before {}", destination_balance_before);

    transfer_spl_token(&payer_signer, &token_mint, amt, &destination, &rpc_client).await;

    let payer_balance_after = get_token_balance_from_token_account(&payer_token_ata, &rpc_client).await;
    println!("Payer token balance after {}", payer_balance_after);
    let destination_balance_after = get_token_balance_from_token_account(&destination_token_ata, &rpc_client).await;
    println!("Destination token balance after {}", destination_balance_after);
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("transfer-spl").
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
            Arg::new("token").
                long("token").
                short('t').
                help("mint of token to transfer").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("destination").
                long("destination").
                short('d').
                help("account to send tokens to").
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