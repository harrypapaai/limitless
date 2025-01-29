use clap::{Arg, ArgMatches, Command};
use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use crate::utils::{get_rpc_client, get_shared_args, get_token_balance_from_token_account, mint_wsol_to_associated_token_account};

pub(crate) async fn get_token_balance(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let account: Pubkey = args.value_of("account").unwrap().parse().unwrap();
    let token_mint: Pubkey = args.value_of("token").unwrap().parse().unwrap();
    let is_token_account = args.is_present("is-token-account");

    let balance = if !is_token_account {
        get_token_balance_from_token_account(&account, &rpc_client).await
    } else {
        let token_account = get_associated_token_address(&account, &token_mint);
        get_token_balance_from_token_account(&token_account, &rpc_client).await
    };

    println!("Balance of token {}: {}", token_mint, balance);
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("get-token-balance").
        args(&get_shared_args()).
        arg(
            Arg::new("account").
                long("account").
                short('a').
                help("account to get balance for").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("is-token-account").
                long("is-token-account").
                short('d').
                help("if the account provided is the token account")
        ).
        arg(
            Arg::new("token").
                long("token").
                short('t').
                help("the mint of the token to get the balance of").
                takes_value(true).
                required(true)
        )
}