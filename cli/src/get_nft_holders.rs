use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use clap::{Arg, ArgMatches, Command};
use solana_program::pubkey::Pubkey;
use crate::helius_utils;

pub(crate) async fn get_nft_owners(args: &ArgMatches) {
    let helius_url = args.value_of("helius-url").unwrap();
    let collection: Pubkey = args.value_of("collection").unwrap().parse().unwrap();
    let output_file = args.value_of("output").unwrap();
    let client = reqwest::Client::new();
    let assets = helius_utils::get_assets_by_group(&client, helius_url, &collection).await.unwrap();
    let mut all_owners: HashSet<String> = HashSet::new();

    for asset in assets.iter() {
        let owner = asset.owner_address.clone();
        all_owners.insert(owner);
    }

    println!("total owners: {:?}", all_owners.len());

    println!("writing to output file: {:?}", output_file);
    let mut file = File::create(output_file).unwrap();
    for line in all_owners {
        writeln!(file, "{}", line).expect("failed to write owner to file");
    }
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("get-nft-holders").
        arg(
            Arg::new("helius-url").
                long("helius-url").
                short('u').
                help("helius url to use").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("collection").
                long("collection").
                short('c').
                help("collection address").
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