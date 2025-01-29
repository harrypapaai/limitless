pub mod pool;
pub mod instructions;

use std::io::Cursor;
use anchor_lang::{InstructionData, ToAccountMetas};
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use spl_associated_token_account::get_associated_token_address;
use borsh::BorshDeserialize;
use crate::{*};

pub async fn exec_init_market(
    t: &mut T,
    creator: &Keypair,
    token_0: &Pubkey,
    token_1: &Pubkey,
    init_amount_0: u64,
    init_amount_1: u64,
) {
    // Give creator some lamports to create accounts.
    token::get_lamports(t, &creator.pubkey(), 1000000000).await;

    let lp_mint_pda = utils::raydium::pda::lp_mint_pda(token_0, token_1).unwrap();
    let (token_0_vault_account, token_1_vault_account) = utils::raydium::pda::token_vault_pdas(token_0, token_1).unwrap();

    let accounts = utils::raydium::raydium_cp_swap::client::accounts::Initialize {
        creator: creator.pubkey(),
        amm_config: utils::raydium::pda::amm_config_pda().unwrap(),
        authority: utils::raydium::pda::authority_pda().unwrap(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0, token_1).unwrap(),
        token0_mint: *token_0,
        token1_mint: *token_1,
        lp_mint: lp_mint_pda,
        creator_token0: get_associated_token_address(&creator.pubkey(), token_0),
        creator_token1: get_associated_token_address(&creator.pubkey(), token_1),
        creator_lp_token: get_associated_token_address(&creator.pubkey(), &lp_mint_pda),
        token0_vault: token_0_vault_account,
        token1_vault: token_1_vault_account,
        create_pool_fee: utils::raydium::CREATE_POOL_FEE_RECEIVER,
        observation_state: utils::raydium::pda::observation_pda(token_0, token_1).unwrap(),
        token_program: spl_token::ID,
        token0_program: spl_token::ID,
        token1_program: spl_token::ID,
        associated_token_program: spl_associated_token_account::ID,
        system_program: solana_program::system_program::ID,
        rent: solana_program::sysvar::rent::ID,
    };
    let init_args = utils::raydium::raydium_cp_swap::client::args::Initialize {
        init_amount0: init_amount_0,
        init_amount1: init_amount_1,
        open_time: 0,
    };
    let data = init_args.data();
    let instr = Instruction::new_with_bytes(
        utils::raydium::ID,
        data.as_slice(),
        accounts.to_account_metas(None),
    );
    t.tx(
        vec![instr],
        vec![&creator],
    ).await;
}

pub async fn exec_deposit(
    t: &mut T,
    user: &Keypair,
    token_0: &Pubkey,
    token_1: &Pubkey,
    lp_token_amt: u64,
    max_token_0_amt: u64,
    max_token_1_amt: u64,
) {
    let instr = instructions::deposit(
        &user.pubkey(),
        token_0,
        token_1,
        lp_token_amt,
        max_token_0_amt,
        max_token_1_amt,
    ).unwrap();
    t.tx(
        vec![instr],
        vec![&user],
    ).await;
}

pub async fn get_price_x32(
    t: &mut T,
    token_0: &Pubkey,
    token_1: &Pubkey,
) -> u128 {
    let pool_state = get_pool_state(t, token_0, token_1).await;
    let (token_0_amount, token_1_account) = get_pool_token_amounts(t, token_0, token_1).await;
    let (price, _) = pool_state.token_price_x32(token_0_amount, token_1_account);
    price
}

pub async fn get_lp_token_balance(
    t: &mut T,
    token_0: &Pubkey,
    token_1: &Pubkey,
    user: &Pubkey,
) -> u64 {
    let lp_mint = utils::raydium::pda::lp_mint_pda(token_0, token_1).unwrap();
    let lp_token_account = get_associated_token_address(user, &lp_mint);
    token::get_token_balance_from_anchor_anchor_token_account(t, &lp_token_account).await
}

pub async fn get_pool_token_amounts(
    t: &mut T,
    token_0: &Pubkey,
    token_1: &Pubkey,
) -> (u64, u64) {
    let (vault_0_account, vault_1_account) = utils::raydium::pda::token_vault_pdas(token_0, token_1).unwrap();
    let token_0_amt = token::get_token_balance_from_anchor_anchor_token_account(t, &vault_0_account).await;
    let token_1_amt = token::get_token_balance_from_anchor_anchor_token_account(t, &vault_1_account).await;
    (token_0_amt, token_1_amt)
}

pub async fn get_pool_state(
    t: &mut T,
    token_0: &Pubkey,
    token_1: &Pubkey,
) -> pool::PoolState {
    let pool_state_pda = utils::raydium::pda::pool_state_pda(token_0, token_1).unwrap();
    let account = t.banks_client().get_account(pool_state_pda.clone()).await.unwrap().unwrap();
    let data_anchor_removed = &account.data[8..];
    let mut data_anchor_removed = data_anchor_removed;
    pool::PoolState::deserialize(&mut data_anchor_removed).unwrap()
}

pub fn load_raydium_amm_admin() -> Keypair {
    let keypair_str = include_str!("../../fixtures/raydium_admin_keypair.json");
    let mut reader = Cursor::new(keypair_str);
    solana_sdk::signature::read_keypair(
        &mut reader
    ).unwrap()
}

pub fn load_raydium_program_account() -> Keypair {
    let keypair_str = include_str!("../../fixtures/raydium_program_account_keypair.json");
    let mut reader = Cursor::new(keypair_str);
    solana_sdk::signature::read_keypair(
        &mut reader
    ).unwrap()
}
