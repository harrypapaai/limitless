use borsh::BorshSerialize;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use utils::instructions::ToAccountMetaList;
use crate::instructions::{InitMarketArgs, DepositLpTokensArgs, LimitlessInstruction, WithdrawLpTokensArgs, AdminInitArgs, MintLpTokensAndDepositArgs, UpdateMarketConfigArgs};
use crate::client::accounts::{admin_init_accounts, close_position_accounts, deposit_liquidity_accounts, edit_position_rollover_accounts, init_market_accounts, open_position_accounts, rollover_position_accounts, update_market_config_accounts, withdraw_liquidity_accounts};
use crate::errors::LimitlessError;
use crate::state::market::QuoteToken;

pub fn admin_init_ix(fee_collector: &Pubkey) -> Result<Instruction, LimitlessError> {
    let accounts = admin_init_accounts(fee_collector)?;
    to_ix(AdminInitArgs{}, accounts.to_account_meta_list())
}

pub fn update_market_config_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: UpdateMarketConfigArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = update_market_config_accounts(
        user_account,
        token_0_mint,
        token_1_mint
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn init_market_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    fee_collector: &Pubkey,
    args: InitMarketArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = init_market_accounts(
        user_account,
        token_0_mint,
        token_1_mint,
        fee_collector,
        args.quote_token,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn deposit_liquidity_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    user_token_0_ta: &Pubkey,
    user_token_1_ta: &Pubkey,
    args: DepositLpTokensArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = deposit_liquidity_accounts(
        user_account,
        token_0_mint,
        token_1_mint,
        user_token_0_ta,
        user_token_1_ta,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn mint_and_deposit_liquidity_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    user_token_0_ta: &Pubkey,
    user_token_1_ta: &Pubkey,
    args: MintLpTokensAndDepositArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = deposit_liquidity_accounts(
        user_account,
        token_0_mint,
        token_1_mint,
        user_token_0_ta,
        user_token_1_ta,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn withdraw_liquidity_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    user_token_0_ta: &Pubkey,
    user_token_1_ta: &Pubkey,
    args: WithdrawLpTokensArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = withdraw_liquidity_accounts(
        user_account,
        token_0_mint,
        token_1_mint,
        user_token_0_ta,
        user_token_1_ta,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn open_position_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: crate::instructions::OpenPositionArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = open_position_accounts(
        user_account,
        token_0_mint,
        token_1_mint,
        &args.id,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn close_position_ix(
    user_account: &Pubkey,
    payer_acount: &Pubkey,
    is_owner: bool,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    fee_collector_account: &Pubkey,
    quote_token: QuoteToken,
    args: crate::instructions::ClosePositionArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = close_position_accounts(
        user_account,
        payer_acount,
        is_owner,
        token_0_mint,
        token_1_mint,
        fee_collector_account,
        quote_token,
        &args.id,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn rollover_position_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    fee_collector: &Pubkey,
    quote_token: QuoteToken,
    args: crate::instructions::RolloverPositionArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = rollover_position_accounts(
        args.id,
        user_account,
        token_0_mint,
        token_1_mint,
        fee_collector,
        quote_token,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

pub fn edit_rollover_position_ix(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    quote_token: QuoteToken,
    args: crate::instructions::EditPositionRolloverArgs,
) -> Result<Instruction, LimitlessError> {
    let accounts = edit_position_rollover_accounts(
        args.id,
        user_account,
        token_0_mint,
        token_1_mint,
        quote_token,
    )?;
    to_ix(args, accounts.to_account_meta_list())
}

fn to_ix<T: Into<LimitlessInstruction>>(
    args: T,
    account_list: Vec<solana_program::instruction::AccountMeta>,
) -> Result<Instruction, LimitlessError> {

    let mut data = Vec::new();
    let instruction: LimitlessInstruction = args.into();
    instruction.serialize(&mut data).map_err(|_| LimitlessError::InvalidInstruction)?;
    Ok(Instruction {
        program_id: crate::ID,
        accounts: account_list,
        data,
    })
}
