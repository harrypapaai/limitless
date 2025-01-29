use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use clap::{Arg, ArgMatches, Command};
use solana_program::pubkey::Pubkey;
use crate::helius_utils;

pub(crate) async fn get_token_holders(args: &ArgMatches) {
    let helius_url = args.value_of("helius-url").unwrap();
    let mint: Pubkey = args.value_of("mint").unwrap().parse().unwrap();
    let output_file = args.value_of("output").unwrap();
    let client = reqwest::Client::new();
    let assets = helius_utils::get_token_holders(&client, helius_url, &mint).await.unwrap();

    let mut vec: Vec<(&String, &u64)> = assets.iter().collect();
    vec.sort_by(|a, b| b.1.cmp(a.1));

    println!("writing to output file: {:?}", output_file);
    let mut file = File::create(output_file).unwrap();
    for (address, _) in vec {
        writeln!(file, "{}", address).expect("failed to write owner to file");
    }

    println!("total owners: {:?}", assets.len());
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("get-token-holders").
        arg(
            Arg::new("helius-url").
                long("helius-url").
                short('u').
                help("helius url to use").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("mint").
                long("mint").
                short('m').
                help("mint address").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("output").
                long("output").
                short('o').
                help("path to output file").
                takes_value(true).
                required(true)
        )
}