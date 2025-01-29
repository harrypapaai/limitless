use std::fs::{read_to_string};
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use crate::utils::{get_rpc_client, get_shared_args};

pub(crate) async fn batch_transfer_tokens(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);

    let token_mint: Pubkey = args.value_of("token").unwrap().parse().unwrap();
    let input_file = args.value_of("input").unwrap();
    let addresses : Vec<String> = read_to_string(input_file).unwrap().lines().map(String::from).collect();
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();
    let payer_ata = get_associated_token_address(&payer_signer.pubkey(), &token_mint);
    let amount : u64 = args.value_of("amount").unwrap().parse().unwrap();

    let mut recipients: Vec<Pubkey> = Vec::new();
    for a in addresses.iter() {
        let account = Pubkey::try_from(a.as_str()).unwrap();
        recipients.push(account);
    }

    for recipient in recipients.iter() {
        let recipient_ata = get_associated_token_address(recipient, &token_mint);
        let create_token_account_idempotent = create_associated_token_account_idempotent(
            &payer_signer.pubkey(), recipient, &token_mint, &spl_token::ID);
        let transfer = spl_token::instruction::transfer(
            &spl_token::ID, &payer_ata, &recipient_ata, &payer_signer.pubkey(), &[], amount).unwrap();
        println!("transferring to: {}", recipient.to_string());
        let mut tx = Transaction::new_with_payer(
            &[create_token_account_idempotent, transfer],
            Some(&payer_signer.pubkey()),
        );
        tx.sign(
            &[payer_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        let signature = rpc_client.send_and_confirm_transaction(&tx).await.unwrap();
        println!("signature: {}", signature.to_string());
    }
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("batch-transfer-tokens").
        args(&get_shared_args()).
        arg(
            Arg::new("payer").
                long("payer").
                short('p').
                help("payer from whom the tokens will come from").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("token").
                long("token").
                short('t').
                help("token mint address").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("input").
                long("input").
                short('i').
                help("path to input file").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("amount").
                long("amount").
                short('a').
                help("amount of tokens").
                takes_value(true).
                required(true)
        )
}