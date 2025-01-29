use std::io::Cursor;
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::token_2022::spl_token_2022;
use solana_program::instruction::Instruction;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_program::rent::Rent;
use solana_sdk::signature::{Keypair, Signer};
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use spl_token::state::Mint;
use utils::errors::UtilsError;

const RAYDIUM_ADMIN_KEYPAIR_STR: &str = include_str!("../../fixtures/raydium_admin_keypair.json");
#[allow(dead_code)]
const RAYDIUM_PROGRAM_ACCOUNT_KEYPAIR_STR: &str = include_str!("../../fixtures/raydium_program_account_keypair.json");
const RAYDIUM_POOL_FEE_RECEIVER_TOKEN_STR: &str = include_str!("../../fixtures/raydium_pool_fee_receiver_token.json");

#[derive(Clone, Copy)]
pub struct RaydiumInitOpts {
    pub trade_fee_rate: Option<u64>,
    pub protocol_fee_rate: Option<u64>,
    pub fund_fee_rate: Option<u64>,
}

const DEFAULT_OPTS: RaydiumInitOpts = RaydiumInitOpts {
    trade_fee_rate: Some(10_000), // 1%
    protocol_fee_rate: Some(0),
    fund_fee_rate: Some(0),
};

pub fn init(payer: &Pubkey, rent: Rent, _opts: Option<RaydiumInitOpts>) -> (Vec<Instruction>, Vec<Keypair>) {
    let opts = if _opts.is_none() {
        DEFAULT_OPTS
    } else {
        _opts.unwrap()
    };
    let admin_kp = get_keypair_from_str(RAYDIUM_ADMIN_KEYPAIR_STR);
    let pool_fee_receiver_token_kp = get_keypair_from_str(RAYDIUM_POOL_FEE_RECEIVER_TOKEN_STR);

    let amm_config_account = utils::raydium::pda::amm_config_pda().unwrap();

    let transfer_ix = solana_program::system_instruction::transfer(
        payer,
        &admin_kp.pubkey(),
        1_000_000_000,
    );

    let init_create_pool_fee_token_account_ix = solana_program::system_instruction::create_account(
        payer,
        &pool_fee_receiver_token_kp.pubkey(),
        rent.minimum_balance(Mint::LEN),
        Mint::LEN as u64,
        &spl_token::id(),
    );

    let init_create_pool_fee_token_mint_ix = spl_token::instruction::initialize_mint(
        &spl_token::id(),
        &pool_fee_receiver_token_kp.pubkey(),
        &pool_fee_receiver_token_kp.pubkey(),
        Some(&pool_fee_receiver_token_kp.pubkey()),
        18,
    ).unwrap();

    let create_pool_fee_receiver_ata_ix = create_associated_token_account_idempotent(
        &payer,
        &admin_kp.pubkey(),
        &pool_fee_receiver_token_kp.pubkey(),
        &spl_token::id(),
    );

    let create_config_accounts = utils::raydium::raydium_cp_swap::client::accounts::CreateAmmConfig {
        owner: admin_kp.pubkey(),
        amm_config: amm_config_account,
        system_program: solana_program::system_program::id(),
    };
    let create_config_args = utils::raydium::raydium_cp_swap::client::args::CreateAmmConfig {
        index: utils::raydium::INDEX,
        trade_fee_rate: opts.trade_fee_rate.unwrap_or(DEFAULT_OPTS.trade_fee_rate.unwrap()),
        protocol_fee_rate: opts.protocol_fee_rate.unwrap_or(DEFAULT_OPTS.trade_fee_rate.unwrap()),
        fund_fee_rate: opts.fund_fee_rate.unwrap_or(DEFAULT_OPTS.trade_fee_rate.unwrap()),
        create_pool_fee: 0
    };
    let create_config_args_data = create_config_args.data();
    let create_config_ix = Instruction::new_with_bytes(
        utils::raydium::ID,
        create_config_args_data.as_slice(),
        create_config_accounts.to_account_metas(None),
    );

    (
        vec![
            transfer_ix,
            init_create_pool_fee_token_account_ix,
            init_create_pool_fee_token_mint_ix,
            create_pool_fee_receiver_ata_ix,
            create_config_ix,
        ],
        vec![admin_kp, pool_fee_receiver_token_kp],
    )
}

pub fn swap_base_input(
    payer: &Pubkey,
    pool_state: &Pubkey,
    input_token_account: &Pubkey,
    output_token_account: &Pubkey,
    input_vault: &Pubkey,
    output_vault: &Pubkey,
    input_token_program: &Pubkey,
    output_token_program: &Pubkey,
    input_token_mint: &Pubkey,
    output_token_mint: &Pubkey,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<Instruction, UtilsError> {

    let (token_0_mint, token_1_mint) = if input_token_mint < output_token_mint {
        (input_token_mint, output_token_mint)
    } else {
        (output_token_mint, input_token_mint)
    };

    let swap_accounts = utils::raydium::raydium_cp_swap::client::accounts::SwapBaseInput {
        payer: *payer,
        authority: utils::raydium::pda::authority_pda()?,
        amm_config: utils::raydium::pda::amm_config_pda()?,
        pool_state: *pool_state,
        input_token_account: *input_token_account,
        output_token_account: *output_token_account,
        input_vault: *input_vault,
        output_vault: *output_vault,
        input_token_program: *input_token_program,
        output_token_program: *output_token_program,
        input_token_mint: *input_token_mint,
        output_token_mint: *output_token_mint,
        observation_state: utils::raydium::pda::observation_pda(token_0_mint, token_1_mint)?,
    };

    let swap_args = utils::raydium::raydium_cp_swap::client::args::SwapBaseInput {
        amount_in,
        minimum_amount_out,
    };
    let swap_args_data = swap_args.data();

    Ok(Instruction::new_with_bytes(
        utils::raydium::ID,
        swap_args_data.as_slice(),
        swap_accounts.to_account_metas(None),
    ))
}

pub fn deposit(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    lp_tokens: u64,
    max_token_0_amt: u64,
    max_token_1_amt: u64,
) -> Result<Instruction, UtilsError> {

    let (token_0_vault, token_1_vault) = utils::raydium::pda::token_vault_pdas(token_0_mint, token_1_mint)?;
    let lp_token_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint)?;
    let user_lp_ata = get_associated_token_address(&user_account, &lp_token_mint);
    let user_token_0_ata = get_associated_token_address(&user_account, token_0_mint);
    let user_token_1_ata = get_associated_token_address(&user_account, token_1_mint);
    let deposit_accounts = utils::raydium::raydium_cp_swap::client::accounts::Deposit {
        owner: *user_account,
        authority: utils::raydium::pda::authority_pda()?,
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint)?,
        owner_lp_token: user_lp_ata,
        token0_account: user_token_0_ata,
        token1_account: user_token_1_ata,
        token0_vault: token_0_vault,
        token1_vault: token_1_vault,
        token_program: spl_token::ID,
        token_program2022: spl_token_2022::ID,
        vault0_mint: *token_0_mint,
        vault1_mint: *token_1_mint,
        lp_mint: lp_token_mint,
    };

    let deposit_args = utils::raydium::raydium_cp_swap::client::args::Deposit {
        lp_token_amount: lp_tokens,
        maximum_token0_amount: max_token_0_amt,
        maximum_token1_amount: max_token_1_amt,
    };
    let deposit_args_data = deposit_args.data();

    Ok(Instruction::new_with_bytes(
        utils::raydium::ID,
        deposit_args_data.as_slice(),
        deposit_accounts.to_account_metas(None),
    ))
}

fn get_keypair_from_str(kp: &str) -> Keypair {
    let mut admin_reader = Cursor::new(kp);
    solana_sdk::signer::keypair::read_keypair(
        &mut admin_reader
    ).unwrap()
}