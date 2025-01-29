use std::str::FromStr;
use anchor_lang::{InstructionData, ToAccountMetas};
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program::sysvar;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use utils::raydium::raydium_cp_swap::client::args::Initialize;
use utils::raydium::raydium_cp_swap::client::accounts::Initialize as InitializeAccounts;
use crate::utils::{get_rpc_client, get_shared_args};

pub(crate) async fn init_pool(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let creator = args.value_of("creator").unwrap();
    let creator_signer = signer_from_path(
        args, creator, "creator", &mut None).unwrap();

    let token0_mint = Pubkey::from_str(args.value_of("token0").unwrap()).unwrap();
    let token1_mint = Pubkey::from_str(args.value_of("token1").unwrap()).unwrap();
    let init_amount0: u64 = 0;
    let init_amount1: u64 = 0;

    let amm_config = utils::raydium::pda::amm_config_pda().unwrap();
    let authority = utils::raydium::pda::authority_pda().unwrap();
    let pool_state = utils::raydium::pda::pool_state_pda(&token0_mint, &token1_mint).unwrap();
    let lp_mint = utils::raydium::pda::lp_mint_pda(&token0_mint, &token1_mint).unwrap();
    let creator_token0 = get_associated_token_address(&creator_signer.pubkey(), &token0_mint);
    let creator_token1 = get_associated_token_address(&creator_signer.pubkey(), &token1_mint);
    let creator_lp_token = get_associated_token_address(&creator_signer.pubkey(), &lp_mint);
    let (token0_vault, token1_vault) = utils::raydium::pda::token_vault_pdas(&token0_mint, &token1_mint).unwrap();
    let observation_state = utils::raydium::pda::observation_pda(&token0_mint, &token1_mint).unwrap();

    let initialize_accounts = InitializeAccounts {
        creator: creator_signer.pubkey(),
        amm_config,
        authority,
        pool_state,
        token0_mint,
        token1_mint,
        lp_mint,
        creator_token0,
        creator_token1,
        creator_lp_token,
        token0_vault,
        token1_vault,
        create_pool_fee: utils::raydium::CREATE_POOL_FEE_RECEIVER,
        observation_state,
        token_program: spl_token::ID,
        token0_program: spl_token::ID,
        token1_program: spl_token::ID,
        associated_token_program: spl_associated_token_account::ID,
        system_program: solana_program::system_program::ID,
        rent: sysvar::rent::ID,
    };

    let initialize_args = Initialize {
        init_amount0,
        init_amount1,
        open_time: 0,
    };

    let initialize_args_data = initialize_args.data();
    let initialize_ix = Instruction::new_with_bytes(
        utils::raydium::ID,
        initialize_args_data.as_slice(),
        initialize_accounts.to_account_metas(None),
    );

    let mut tx = Transaction::new_with_payer(
        &[initialize_ix],
        Some(&creator_signer.pubkey()),
    );
    tx.sign(
        &[creator_signer.as_ref()],
        rpc_client.get_latest_blockhash().await.unwrap(),
    );
    let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not create pool");
    println!("created pool, signature: {}", signature);
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("raydium-init-pool").
        args(&get_shared_args()).
        arg(
            Arg::new("creator").
                long("creator").
                short('c').
                help("path to the creator who is initializing the pool").
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
        )
}