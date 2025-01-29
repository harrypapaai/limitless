use std::str::FromStr;
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::pubkey::Pubkey;
use crate::utils::{self, get_rpc_client, get_shared_args};
const SENTINAL: &str = "sentinal";

pub(crate) async fn create_token_account(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();

    let token_mint: Pubkey = args.value_of("token").unwrap().parse().unwrap();

    let owner_key_raw = args.value_of("owner").unwrap();
    let owner : Pubkey;
    if owner_key_raw == SENTINAL {
        owner = payer_signer.pubkey()
    } else {
        owner = Pubkey::from_str(owner_key_raw).unwrap();
    }

    utils::create_token_account(&payer_signer, &owner, &token_mint, &rpc_client).await;
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("create-token-account").
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
                help("mint of token to create account for").
                takes_value(true).
                required(true)
        ).arg(
            Arg::new("owner").
                long("owner").
                short('o').
                help("owner address for token account").
                takes_value(true).
                default_value(SENTINAL)
        )
}
