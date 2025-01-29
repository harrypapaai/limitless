use std::str::FromStr;
use anchor_lang::{InstructionData, ToAccountMetas};
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use utils::raydium::raydium_cp_swap::client::args::Deposit;
use anchor_spl::token_2022::spl_token_2022;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use utils::raydium::raydium_cp_swap::client::accounts::Deposit as DepositAccounts;
use crate::utils::{get_rpc_client, get_shared_args};

pub(crate) async fn raydium_deposit(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let creator = args.value_of("creator").unwrap();
    let creator_signer = signer_from_path(
        args, creator, "creator", &mut None).unwrap();

    let token0_mint = Pubkey::from_str(args.value_of("token0").unwrap()).unwrap();
    let token1_mint = Pubkey::from_str(args.value_of("token1").unwrap()).unwrap();
    let lp_token_amount: u64 = args.value_of("lp_token_amount").unwrap().parse().unwrap();
    let maximum_token0_amount: u64 = args.value_of("max_token_0_amount").unwrap().parse().unwrap();
    let maximum_token1_amount: u64 = args.value_of("max_token_1_amount").unwrap().parse().unwrap();

    let authority = utils::raydium::pda::authority_pda().unwrap();
    let pool_state = utils::raydium::pda::pool_state_pda(&token0_mint, &token1_mint).unwrap();
    let lp_mint = utils::raydium::pda::lp_mint_pda(&token0_mint, &token1_mint).unwrap();
    let creator_token0 = get_associated_token_address(&creator_signer.pubkey(), &token0_mint);
    let creator_token1 = get_associated_token_address(&creator_signer.pubkey(), &token1_mint);
    let creator_lp_token = get_associated_token_address(&creator_signer.pubkey(), &lp_mint);
    let (token0_vault, token1_vault) = utils::raydium::pda::token_vault_pdas(&token0_mint, &token1_mint).unwrap();

    let create_creator_lp_token_account_ix = create_associated_token_account_idempotent(
        &creator_signer.pubkey(),
        &creator_signer.pubkey(),
        &lp_mint,
        &spl_token::ID,
    );

    let deposit_accounts = DepositAccounts {
        owner: creator_signer.pubkey(),
        authority,
        pool_state,
        owner_lp_token: creator_lp_token,
        token0_account: creator_token0,
        token1_account: creator_token1,
        lp_mint,
        token0_vault,
        token1_vault,
        token_program: spl_token::ID,
        token_program2022: spl_token_2022::ID,
        vault0_mint: token0_mint,
        vault1_mint: token1_mint,
    };

    let deposit_args = Deposit {
        lp_token_amount,
        maximum_token0_amount,
        maximum_token1_amount,
    };

    let deposit_args_data = deposit_args.data();
    let deposit_ix = Instruction::new_with_bytes(
        utils::raydium::ID,
        deposit_args_data.as_slice(),
        deposit_accounts.to_account_metas(None),
    );

    let mut tx = Transaction::new_with_payer(
        &[create_creator_lp_token_account_ix, deposit_ix],
        Some(&creator_signer.pubkey()),
    );
    tx.sign(
        &[creator_signer.as_ref()],
        rpc_client.get_latest_blockhash().await.unwrap(),
    );
    let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not deposit liquidity");
    println!("deposited liquidity, signature: {}", signature);
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("raydium-deposit").
        args(&get_shared_args()).
        arg(
            Arg::new("creator").
                long("creator").
                short('c').
                help("path to the creator who is depositing into the pool").
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
                required(true)
        ).
        arg(
            Arg::new("max_token_0_amount").
                long("max_token_0_amount").
                help("max amount of token 0 to deposit").
                takes_value(true).
                required(false).
                default_value("1000000000000000000")
        ).
        arg(
            Arg::new("token1").
                long("token1").
                help("max amount of token 1 to deposit").
                takes_value(true).
                required(false).
                default_value("1000000000000000000")
        )
}