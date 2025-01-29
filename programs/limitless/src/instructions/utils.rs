use std::ops::DerefMut;
use anchor_lang::context::CpiContext;
use solana_program::pubkey::Pubkey;
use utils::numbers::SafeUnsigned;
use utils::raydium::raydium_cp_swap;
use solana_program::account_info::AccountInfo;
use solana_program::program::{invoke, invoke_signed};
use spl_token::instruction::sync_native;
use utils::events::EventAuthoritySignerSeeds;
use utils::log;
use utils::state::Packable;
use utils::token::create_token_account_with_signers;
use utils::validate::validate_ata;
use crate::{calculator, state};
use crate::calculator::redeposit_fees_calc;
use crate::errors::LimitlessError;
use crate::events::{emit_cpi_ix, MarketStateUpdateEvent, RolloverPositionEvent};
use crate::state::config::ConfigAccount;
use crate::state::liquidity_position::LiquidityPositionAccount;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, MarketIntermediateTokenAccountSignerSeeds, MarketState, QuoteToken};
use crate::state::position::PositionAccount;

//
// Swapping utils.
//

pub struct MarketSwapAccounts<'refs, 'info> {
    pub market_account: &'refs AccountInfo<'info>,
    pub market_token_0_ata: &'refs AccountInfo<'info>,
    pub market_token_1_ata: &'refs AccountInfo<'info>,

    pub token_0_mint: &'refs AccountInfo<'info>,
    pub token_1_mint: &'refs AccountInfo<'info>,
    pub token_0_vault: &'refs AccountInfo<'info>,
    pub token_1_vault: &'refs AccountInfo<'info>,
    pub token_program: &'refs AccountInfo<'info>,
    pub raydium_program: &'refs AccountInfo<'info>,
    pub raydium_config: &'refs AccountInfo<'info>,
    pub pool_state: &'refs AccountInfo<'info>,
    pub pool_authority: &'refs AccountInfo<'info>,
    pub pool_observation: &'refs AccountInfo<'info>,
}

pub fn market_swap_exact_input(
    market_account_signer_seeds: &MarketAccountSignerSeeds,
    account_infos: &MarketSwapAccounts,
    token_to_swap_mint: &Pubkey,
    token_input_amt: u64,
    min_token_output_amt: u64,
) -> Result<u64, LimitlessError> {
    let before_token_0_market_balance = utils::token::amount_from_token_account_info(account_infos.market_token_0_ata)?;
    let before_token_1_market_balance = utils::token::amount_from_token_account_info(account_infos.market_token_1_ata)?;

    let swap_token_accounts = get_swap_token_accounts(
        token_to_swap_mint,
        account_infos.token_0_mint,
        account_infos.token_1_mint,
        account_infos.token_0_vault,
        account_infos.token_1_vault,
        account_infos.market_token_0_ata,
        account_infos.market_token_1_ata,
    );
    raydium_cp_swap::cpi::swap_base_input(
        CpiContext::new_with_signer(
            account_infos.raydium_program.clone(),
            raydium_cp_swap::cpi::accounts::SwapBaseInput{
                payer: account_infos.market_account.clone(),
                authority: account_infos.pool_authority.clone(),
                amm_config: account_infos.raydium_config.clone(),
                pool_state: account_infos.pool_state.clone(),
                input_token_account: swap_token_accounts.token_to_swap_source_acc.clone(),
                output_token_account: swap_token_accounts.token_to_receive_dest_acc.clone(),
                input_vault: swap_token_accounts.token_to_swap_pool_vault.clone(),
                output_vault: swap_token_accounts.token_to_receive_pool_vault.clone(),
                input_token_program: account_infos.token_program.clone(),
                output_token_program: account_infos.token_program.clone(),
                input_token_mint: swap_token_accounts.token_to_swap_mint.clone(),
                output_token_mint: swap_token_accounts.token_to_receive_mint.clone(),
                observation_state: account_infos.pool_observation.clone(),
            },
            &[&market_account_signer_seeds.as_refs()]
        ),
        token_input_amt,
        min_token_output_amt,
    ).map_err(|_| LimitlessError::SwapInvokeFailed)?;

    // Get actual amount of token swapped out.
    let after_token_0_market_balance = utils::token::amount_from_token_account_info(account_infos.market_token_0_ata)?;
    let after_token_1_market_balance = utils::token::amount_from_token_account_info(account_infos.market_token_1_ata)?;

    let result = if token_to_swap_mint.eq(account_infos.token_0_mint.key) {
        let token_0_balance_check = before_token_0_market_balance.safe_sub(token_input_amt)?;
        if after_token_0_market_balance != token_0_balance_check {
            return Err(LimitlessError::InvalidToken0Input);
        };
        after_token_1_market_balance.safe_sub(before_token_1_market_balance)?
    } else {
        let token_1_balance_check = before_token_1_market_balance.safe_sub(token_input_amt)?;
        if after_token_1_market_balance != token_1_balance_check {
            return Err(LimitlessError::InvalidToken1Input);
        };
        after_token_0_market_balance.safe_sub(before_token_0_market_balance)?
    };

    Ok(result)
}

pub struct UserSwapAccounts<'refs, 'info> {
    pub user_account: &'refs AccountInfo<'info>,
    pub user_token_0_ata: &'refs AccountInfo<'info>,
    pub user_token_1_ata: &'refs AccountInfo<'info>,

    pub token_0_mint: &'refs AccountInfo<'info>,
    pub token_1_mint: &'refs AccountInfo<'info>,
    pub token_0_vault: &'refs AccountInfo<'info>,
    pub token_1_vault: &'refs AccountInfo<'info>,
    pub token_program: &'refs AccountInfo<'info>,
    pub raydium_program: &'refs AccountInfo<'info>,
    pub raydium_config: &'refs AccountInfo<'info>,
    pub pool_state: &'refs AccountInfo<'info>,
    pub pool_authority: &'refs AccountInfo<'info>,
    pub pool_observation: &'refs AccountInfo<'info>,
}

#[allow(dead_code)]
pub fn user_swap_exact_output(
    account_infos: &UserSwapAccounts,
    token_to_swap_mint: &Pubkey,
    token_output_amt: u64,
    max_token_input_amt: u64,
) -> Result<u64, LimitlessError> {
    let before_token_0_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_0_ata)?;
    let before_token_1_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_1_ata)?;

    let swap_token_accounts = get_swap_token_accounts(
        token_to_swap_mint,
        account_infos.token_0_mint,
        account_infos.token_1_mint,
        account_infos.token_0_vault,
        account_infos.token_1_vault,
        account_infos.user_token_0_ata,
        account_infos.user_token_1_ata,
    );
    raydium_cp_swap::cpi::swap_base_output(
        CpiContext::new(
            account_infos.raydium_program.clone(),
            raydium_cp_swap::cpi::accounts::SwapBaseOutput{
                payer: account_infos.user_account.clone(),
                authority: account_infos.pool_authority.clone(),
                amm_config: account_infos.raydium_config.clone(),
                pool_state: account_infos.pool_state.clone(),
                input_token_account: swap_token_accounts.token_to_swap_source_acc.clone(),
                output_token_account: swap_token_accounts.token_to_receive_dest_acc.clone(),
                input_vault: swap_token_accounts.token_to_swap_pool_vault.clone(),
                output_vault: swap_token_accounts.token_to_receive_pool_vault.clone(),
                input_token_program: account_infos.token_program.clone(),
                output_token_program: account_infos.token_program.clone(),
                input_token_mint: swap_token_accounts.token_to_swap_mint.clone(),
                output_token_mint: swap_token_accounts.token_to_receive_mint.clone(),
                observation_state: account_infos.pool_observation.clone(),
            },
        ),
        max_token_input_amt,
        token_output_amt,
    ).map_err(|_| LimitlessError::SwapInvokeFailed)?;

    // Get actual amount of token swapped out.
    let after_token_0_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_0_ata)?;
    let after_token_1_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_1_ata)?;

    let result = if token_to_swap_mint.eq(account_infos.token_1_mint.key) {
        let token_0_balance_check = before_token_0_user_balance.safe_add(token_output_amt)?;
        if after_token_0_user_balance != token_0_balance_check {
            return Err(LimitlessError::InvalidToken0Output);
        };
        after_token_1_user_balance.safe_sub(before_token_0_user_balance)?
    } else {
        let token_1_balance_check = before_token_1_user_balance.safe_add(token_output_amt)?;
        if after_token_1_user_balance != token_1_balance_check {
            return Err(LimitlessError::InvalidToken1Output);
        };
        after_token_0_user_balance.safe_sub(before_token_0_user_balance)?
    };

    Ok(result)
}

pub fn user_swap_exact_input(
    account_infos: &UserSwapAccounts,
    token_to_swap_mint: &Pubkey,
    token_input_amt: u64,
    min_token_output_amt: u64,
) -> Result<u64, LimitlessError> {
    let before_token_0_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_0_ata)?;
    let before_token_1_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_1_ata)?;

    let swap_token_accounts = get_swap_token_accounts(
        token_to_swap_mint,
        account_infos.token_0_mint,
        account_infos.token_1_mint,
        account_infos.token_0_vault,
        account_infos.token_1_vault,
        account_infos.user_token_0_ata,
        account_infos.user_token_1_ata,
    );
    raydium_cp_swap::cpi::swap_base_input(
        CpiContext::new(
            account_infos.raydium_program.clone(),
            raydium_cp_swap::cpi::accounts::SwapBaseInput{
                payer: account_infos.user_account.clone(),
                authority: account_infos.pool_authority.clone(),
                amm_config: account_infos.raydium_config.clone(),
                pool_state: account_infos.pool_state.clone(),
                input_token_account: swap_token_accounts.token_to_swap_source_acc.clone(),
                output_token_account: swap_token_accounts.token_to_receive_dest_acc.clone(),
                input_vault: swap_token_accounts.token_to_swap_pool_vault.clone(),
                output_vault: swap_token_accounts.token_to_receive_pool_vault.clone(),
                input_token_program: account_infos.token_program.clone(),
                output_token_program: account_infos.token_program.clone(),
                input_token_mint: swap_token_accounts.token_to_swap_mint.clone(),
                output_token_mint: swap_token_accounts.token_to_receive_mint.clone(),
                observation_state: account_infos.pool_observation.clone(),
            },
        ),
        token_input_amt,
        min_token_output_amt,
    ).map_err(|_| LimitlessError::SwapInvokeFailed)?;

    // Get actual amount of token swapped out.
    let after_token_0_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_0_ata)?;
    let after_token_1_user_balance = utils::token::amount_from_token_account_info(account_infos.user_token_1_ata)?;

    let result = if token_to_swap_mint.eq(account_infos.token_0_mint.key) {
        let token_0_balance_check = before_token_0_user_balance.safe_sub(token_input_amt)?;
        if after_token_0_user_balance != token_0_balance_check {
            return Err(LimitlessError::InvalidToken0Input);
        };
        after_token_1_user_balance.safe_sub(before_token_1_user_balance)?
    } else {
        let token_1_balance_check = before_token_1_user_balance.safe_sub(token_input_amt)?;
        if after_token_1_user_balance != token_1_balance_check {
            return Err(LimitlessError::InvalidToken1Input);
        };
        after_token_0_user_balance.safe_sub(before_token_0_user_balance)?
    };

    Ok(result)
}

//
// Transfer utils.
//

pub fn wrap_wsol_if_needed<'info>(
    user_info: &AccountInfo<'info>,
    user_ata_info: &AccountInfo<'info>,
    token_mint: &Pubkey,
    amount: u64,
) -> Result<(), LimitlessError> {
    if token_mint.eq(&spl_token::native_mint::ID) {
        let balance_before = utils::token::amount_from_token_account_info(user_ata_info)?;
        if balance_before < amount {
            let difference = amount - balance_before;
            invoke(
                &solana_program::system_instruction::transfer(
                    &user_info.key,
                    &user_ata_info.key,
                    difference,
                ),
                &[
                    user_info.clone(),
                    user_ata_info.clone(),
                ],
            ).map_err(|_| LimitlessError::WsolMintFailed)?;
            invoke(
                &sync_native(
                    &spl_token::ID,
                    &user_ata_info.key,
                ).map_err(|_| LimitlessError::WsolMintFailed)?,
                &[
                    user_ata_info.clone(),
                ],
            ).map_err(|_| LimitlessError::WsolMintFailed)?;
        }
    }

    Ok(())
}

pub fn transfer_spl_from_user_to_market<'info>(
    user_info: &AccountInfo<'info>,
    user_ata_info: &AccountInfo<'info>,
    market_ata_info: &AccountInfo<'info>,
    token_program_info: &AccountInfo<'info>,
    amount: u64,
) -> Result<(), LimitlessError> {
    invoke(
        &spl_token::instruction::transfer(
            token_program_info.key,
            user_ata_info.key,
            market_ata_info.key,
            user_info.key,
            &[],
            amount,
        ).map_err(|_| LimitlessError::SplTransferInstructionSerializationFailed)?,
        &[
            user_ata_info.clone(),
            market_ata_info.clone(),
            user_info.clone(),
        ],
    ).map_err(|_| LimitlessError::SplTransferInvokeFailed)
}

pub fn transfer_spl_from_market_to_user_unwrap_wsol<'info>(
    market_account_signer_seeds: &MarketAccountSignerSeeds,
    market_intermediate_wsol_ta_signer_seeds: &MarketIntermediateTokenAccountSignerSeeds,
    market_account_info: &AccountInfo<'info>,
    market_ata_info: &AccountInfo<'info>,
    user_ata_info: &AccountInfo<'info>,
    user_info: &AccountInfo<'info>,
    payer_info: &AccountInfo<'info>,
    token_mint: &AccountInfo<'info>,
    intermediate_ta_info: &AccountInfo<'info>,
    token_program_info: &AccountInfo<'info>,
    system_program_info: &AccountInfo<'info>,
    rent_info: &AccountInfo<'info>,
    amount: u64,
) -> Result<(), LimitlessError> {
    if token_mint.key.eq(&spl_token::native_mint::ID) {
        create_token_account_with_signers(
            intermediate_ta_info,
            token_mint,
            market_account_info,
            payer_info,
            token_program_info,
            system_program_info,
            rent_info,
            &market_intermediate_wsol_ta_signer_seeds.as_refs(),
            &market_account_signer_seeds.as_refs(),
        ).map_err(|_| LimitlessError::CreateTokenAccountInvokeFailed)?;

        transfer_spl_from_market_to_user(
            market_account_signer_seeds,
            market_account_info,
            market_ata_info,
            intermediate_ta_info,
            token_program_info,
            amount
        )?;

        // if the payer is not the user, we unwrap into the payers account, then transfer to the user
        // this allows for the rent paid for the token account to be returned to the payer
        let destination_info: &AccountInfo;
        if payer_info.key.eq(user_info.key) {
            destination_info = user_info;
        } else {
            destination_info = payer_info;
        }

        invoke_signed(
            &spl_token::instruction::close_account(
                token_program_info.key,
                intermediate_ta_info.key,
                destination_info.key,
                market_account_info.key,
                &[],
            ).map_err(|_| LimitlessError::SplTransferInvokeFailed)?,
            &[
                intermediate_ta_info.clone(),
                destination_info.clone(),
                market_account_info.clone(),
            ],
            &[
                &market_account_signer_seeds.as_refs(),
            ],
        ).map_err(|_| LimitlessError::SplTransferInvokeFailed)?;

        if !payer_info.key.eq(user_info.key) {
            invoke(
                &solana_program::system_instruction::transfer(
                    &payer_info.key,
                    &user_info.key,
                    amount,
                ),
                &[
                    payer_info.clone(),
                    user_info.clone(),
                ],
            ).map_err(|_| LimitlessError::SplTransferInvokeFailed)?;
        }

        Ok(())
    } else {
        transfer_spl_from_market_to_user(
            market_account_signer_seeds,
            market_account_info,
            market_ata_info,
            user_ata_info,
            token_program_info,
            amount
        )
    }
}

pub fn transfer_spl_from_market_to_user<'info>(
    market_account_signer_seeds: &MarketAccountSignerSeeds,
    market_account_info: &AccountInfo<'info>,
    market_ata_info: &AccountInfo<'info>,
    user_ata_info: &AccountInfo<'info>,
    token_program_info: &AccountInfo<'info>,
    amount: u64,
) -> Result<(), LimitlessError> {
    invoke_signed(
        &spl_token::instruction::transfer(
            token_program_info.key,
            market_ata_info.key,
            user_ata_info.key,
            market_account_info.key,
            &[market_account_info.key],
            amount,
        ).map_err(|_| LimitlessError::SplTransferInstructionSerializationFailed)?,
        &[
            market_ata_info.clone(),
            user_ata_info.clone(),
            market_account_info.clone(),
            market_account_info.clone(),
        ],
        &[
            &market_account_signer_seeds.as_refs(),
        ],
    ).map_err(|_| LimitlessError::SplTransferInvokeFailed)
}

//
// Mint and burn LP helpers
//

pub struct UserMintLpTokenAccounts<'refs, 'info> {
    pub user_account: &'refs AccountInfo<'info>,
    pub user_token_0_ata: &'refs AccountInfo<'info>,
    pub user_token_1_ata: &'refs AccountInfo<'info>,
    pub user_raydium_lp_token_ata: &'refs AccountInfo<'info>,

    pub token_0_mint: &'refs AccountInfo<'info>,
    pub token_1_mint: &'refs AccountInfo<'info>,
    pub raydium_lp_mint: &'refs AccountInfo<'info>,
    pub token_0_vault: &'refs AccountInfo<'info>,
    pub token_1_vault: &'refs AccountInfo<'info>,
    pub token_program: &'refs AccountInfo<'info>,
    pub token_program2022: &'refs AccountInfo<'info>,
    pub raydium_program: &'refs AccountInfo<'info>,
    pub pool_state: &'refs AccountInfo<'info>,
    pub pool_authority: &'refs AccountInfo<'info>,
}

pub fn mint_lp_tokens_for_user<'info>(
    account_infos: &UserMintLpTokenAccounts,
    lp_token_amt: u64,
    max_token_0_amt: u64,
    max_token_1_amt: u64,
) -> Result<(u64, u64), LimitlessError> {
    let user_token_0_balance_before = utils::token::amount_from_token_account_info(account_infos.user_token_0_ata)?;
    let user_token_1_balance_before = utils::token::amount_from_token_account_info(account_infos.user_token_1_ata)?;

    raydium_cp_swap::cpi::deposit(
        CpiContext::new(
            account_infos.raydium_program.clone(),
            raydium_cp_swap::cpi::accounts::Deposit{
                owner: account_infos.user_account.clone(),
                authority: account_infos.pool_authority.clone(),
                pool_state: account_infos.pool_state.clone(),
                owner_lp_token: account_infos.user_raydium_lp_token_ata.clone(),
                token0_account: account_infos.user_token_0_ata.clone(),
                token1_account: account_infos.user_token_1_ata.clone(),
                token0_vault: account_infos.token_0_vault.clone(),
                token1_vault: account_infos.token_1_vault.clone(),
                token_program: account_infos.token_program.clone(),
                token_program2022: account_infos.token_program2022.clone(),
                vault0_mint: account_infos.token_0_mint.clone(),
                vault1_mint: account_infos.token_1_mint.clone(),
                lp_mint: account_infos.raydium_lp_mint.clone(),
            },
        ),
        lp_token_amt,
        max_token_0_amt,
        max_token_1_amt,
    ).map_err(|_| LimitlessError::LpTokenMintInvokeFailed)?;

    let user_token_0_balance_after = utils::token::amount_from_token_account_info(account_infos.user_token_0_ata)?;
    let user_token_1_balance_after = utils::token::amount_from_token_account_info(account_infos.user_token_1_ata)?;

    Ok((
        user_token_0_balance_before.safe_sub(user_token_0_balance_after)?,
        user_token_1_balance_before.safe_sub(user_token_1_balance_after)?,
    ))
}

pub struct MarketMintLpTokenAccounts<'refs, 'info> {
    pub market_account: &'refs AccountInfo<'info>,
    pub market_token_0_ata: &'refs AccountInfo<'info>,
    pub market_token_1_ata: &'refs AccountInfo<'info>,
    pub market_raydium_lp_token_ata: &'refs AccountInfo<'info>,

    pub token_0_mint: &'refs AccountInfo<'info>,
    pub token_1_mint: &'refs AccountInfo<'info>,
    pub raydium_lp_mint: &'refs AccountInfo<'info>,
    pub token_0_vault: &'refs AccountInfo<'info>,
    pub token_1_vault: &'refs AccountInfo<'info>,
    pub token_program: &'refs AccountInfo<'info>,
    pub token_program2022: &'refs AccountInfo<'info>,
    pub raydium_program: &'refs AccountInfo<'info>,
    pub pool_state: &'refs AccountInfo<'info>,
    pub pool_authority: &'refs AccountInfo<'info>,
}

pub fn mint_lp_tokens_for_market<'info>(
    market_account_signer_seeds: &MarketAccountSignerSeeds,
    account_infos: &MarketMintLpTokenAccounts,
    lp_token_amt: u64,
    max_token_0_amt: u64,
    max_token_1_amt: u64,
) -> Result<(u64, u64), LimitlessError> {
    let market_token_0_balance_before = utils::token::amount_from_token_account_info(account_infos.market_token_0_ata)?;
    let market_token_1_balance_before = utils::token::amount_from_token_account_info(account_infos.market_token_1_ata)?;

    raydium_cp_swap::cpi::deposit(
        CpiContext::new_with_signer(
            account_infos.raydium_program.clone(),
            raydium_cp_swap::cpi::accounts::Deposit{
                owner: account_infos.market_account.clone(),
                authority: account_infos.pool_authority.clone(),
                pool_state: account_infos.pool_state.clone(),
                owner_lp_token: account_infos.market_raydium_lp_token_ata.clone(),
                token0_account: account_infos.market_token_0_ata.clone(),
                token1_account: account_infos.market_token_1_ata.clone(),
                token0_vault: account_infos.token_0_vault.clone(),
                token1_vault: account_infos.token_1_vault.clone(),
                token_program: account_infos.token_program.clone(),
                token_program2022: account_infos.token_program2022.clone(),
                vault0_mint: account_infos.token_0_mint.clone(),
                vault1_mint: account_infos.token_1_mint.clone(),
                lp_mint: account_infos.raydium_lp_mint.clone(),
            },
            &[&market_account_signer_seeds.as_refs()],
        ),
        lp_token_amt,
        max_token_0_amt,
        max_token_1_amt,
    ).map_err(|_| LimitlessError::LpTokenMintInvokeFailed)?;

    let market_token_0_balance_after = utils::token::amount_from_token_account_info(account_infos.market_token_0_ata)?;
    let market_token_1_balance_after = utils::token::amount_from_token_account_info(account_infos.market_token_1_ata)?;

    Ok((
        market_token_0_balance_before.safe_sub(market_token_0_balance_after)?,
        market_token_1_balance_before.safe_sub(market_token_1_balance_after)?,
    ))
}

//
// Fee utils.
//

pub fn validate_fee_collector_ata<'a, 'b>(
    fee_collector_quote_token_ata_info: &'a AccountInfo<'b>,
    market_account: &MarketAccount,
    config: &ConfigAccount,
    token_0_mint_info: &'a AccountInfo<'b>,
    token_1_mint_info: &'a AccountInfo<'b>,
    token_program_info: &'a AccountInfo<'b>,
) -> Result<(), LimitlessError> {
    let quote_token_mint_info = match market_account.quote_token {
        QuoteToken::Token1 => token_1_mint_info,
        QuoteToken::Token0 => token_0_mint_info,
    };
    validate_ata(
        fee_collector_quote_token_ata_info,
        quote_token_mint_info,
        token_program_info,
        &config.fee_collector,
        LimitlessError::InvalidFeeCollectorAta
    )
}

pub fn redeem_liquidity_position_fees<'info>(
    liquidity_position_account: &mut LiquidityPositionAccount,
    market_account: &mut MarketAccount,

    market_account_signer_seeds: &MarketAccountSignerSeeds,
    liquidity_position_account_info: &AccountInfo<'info>,
    market_account_info: &AccountInfo<'info>,
    market_token_0_ata_info: &AccountInfo<'info>,
    market_token_1_ata_info: &AccountInfo<'info>,
    user_token_0_ata_info: &AccountInfo<'info>,
    user_token_1_ata_info: &AccountInfo<'info>,
    token_program_info: &AccountInfo<'info>,
) -> Result<(), LimitlessError> {
    // LP tokens.
    // Will redeem the LP tokens earned as fees and add it to the existing LP token position.
    let lp_token_fees = if liquidity_position_account.lp_token_fee_pool_position.fake_and_real_share_token_amt()? > 0 {
        let lp_token_fees = market_account.lp_token_fee_balance_pool
            .redeem_position(&mut liquidity_position_account.lp_token_fee_pool_position)?;
        if lp_token_fees > 0 {
            market_account.lp_tokens_supplied_pool
                .incr_position_amt(&mut liquidity_position_account.lp_token_pool_position, lp_token_fees)?;
        };
        lp_token_fees
    } else {
        0
    };

    // Token 0.
    let token_0_fees = if liquidity_position_account.token_0_fee_pool_position.fake_and_real_share_token_amt()? > 0 {
        let token_0_fees = market_account.token_0_fee_balance_pool
            .redeem_position(&mut liquidity_position_account.token_0_fee_pool_position)?;
        if token_0_fees > 0 {
            transfer_spl_from_market_to_user(
                market_account_signer_seeds,
                market_account_info,
                market_token_0_ata_info,
                user_token_0_ata_info,
                token_program_info,
                token_0_fees,
            )?;
        }
        token_0_fees
    } else {
        0
    };

    // Token 1.
    let token_1_fees = if liquidity_position_account.token_1_fee_pool_position.fake_and_real_share_token_amt()? > 0 {
        let token_1_fees = market_account.token_1_fee_balance_pool
            .redeem_position(&mut liquidity_position_account.token_1_fee_pool_position)?;
        if token_1_fees > 0 {
            transfer_spl_from_market_to_user(
                market_account_signer_seeds,
                market_account_info,
                market_token_1_ata_info,
                user_token_1_ata_info,
                token_program_info,
                token_1_fees,
            )?;
        }
        token_1_fees
    } else {
        0
    };

    if lp_token_fees > 0 || token_0_fees > 0 || token_1_fees > 0 {
        liquidity_position_account.pack(liquidity_position_account_info)
            .map_err(|_| LimitlessError::LiquidityPositionAccountSerializationFailed)?;
    };

    Ok(())
}

pub fn deposit_fees_as_liquidity<'info>(
    market_account: &mut MarketAccount,
    raydium_base_token_pool_amt: u64,
    raydium_quote_token_pool_amt: u64,
    raydium_lp_token_supply: u64,
    raydium_trade_fee_rate: u64,
    raydium_protocol_fee_rate: u64,
    raydium_fund_fee_rate: u64,
    quote_token_mint: &Pubkey,

    market_account_signer_seeds: &MarketAccountSignerSeeds,
    market_swap_accounts: &MarketSwapAccounts,
    market_mint_lp_token_accounts: &MarketMintLpTokenAccounts,
) -> Result<(u64, u64, u64), LimitlessError> {
    let (base_token_balance, quote_token_balance) = match market_account.quote_token {
        QuoteToken::Token1 => (
            market_account.token_0_fee_balance_pool.total_balance(),
            market_account.token_1_fee_balance_pool.total_balance(),
        ),
        QuoteToken::Token0 => (
            market_account.token_1_fee_balance_pool.total_balance(),
            market_account.token_0_fee_balance_pool.total_balance(),
        )
    };

    let (
        expected_lp_tokens_minted,
        base_tokens_to_deposit,
        quote_tokens_to_deposit,
    ) = {
        let res = redeposit_fees_calc(
            base_token_balance,
            quote_token_balance,
            raydium_base_token_pool_amt,
            raydium_quote_token_pool_amt,
            raydium_lp_token_supply,
            raydium_trade_fee_rate,
            raydium_protocol_fee_rate,
            raydium_fund_fee_rate,
        );
        if res.is_ok_and(|calcs| calcs.quote_token_to_swap > 0) {
            let calcs = res.unwrap();
            log!(
                "Swapping {} quote for {} base to deposit as LP",
                calcs.quote_token_to_swap,
                calcs.expected_base_token_received,
            );
            market_swap_exact_input(
                market_account_signer_seeds,
                market_swap_accounts,
                quote_token_mint,
                calcs.quote_token_to_swap,
                calcs.expected_base_token_received,
            )?;
            // Reflect swap in fee balance pools.
            match market_account.quote_token {
                QuoteToken::Token1 => {
                    market_account.token_0_fee_balance_pool.incr_balance(calcs.expected_base_token_received)?;
                    market_account.token_1_fee_balance_pool.decr_balance(calcs.quote_token_to_swap)?;
                }
                QuoteToken::Token0 => {
                    market_account.token_0_fee_balance_pool.decr_balance(calcs.quote_token_to_swap)?;
                    market_account.token_1_fee_balance_pool.incr_balance(calcs.expected_base_token_received)?;
                }
            }
            (
                calcs.expected_lp_tokens_minted,
                calcs.base_token_to_deposit,
                calcs.quote_token_to_deposit,
            )
        } else {
            log!("Not swapping fees because swapping will not result in enough tokens to redeposit for LP");
            if res.is_err() {
                log!("Redeposit Fee Calculation Error: {:?}", res.unwrap_err());
            }
            let (
                lp_tokens_minted,
                base_tokens_used,
                quote_tokens_used,
            ) = calculator::calculate_lp_token_amt_rounded_down_u128(
                raydium_lp_token_supply as u128,
                raydium_base_token_pool_amt as u128,
                raydium_quote_token_pool_amt as u128,
                base_token_balance as u128,
                quote_token_balance as u128,
            )?;
            (
                lp_tokens_minted.try_into()?,
                base_tokens_used.try_into()?,
                quote_tokens_used.try_into()?,
            )
        }
    };

    let (token_0_amt, token_1_amt) = match market_account.quote_token {
        QuoteToken::Token1 => (base_tokens_to_deposit, quote_tokens_to_deposit),
        QuoteToken::Token0 => (quote_tokens_to_deposit, base_tokens_to_deposit),
    };

    if expected_lp_tokens_minted > 0 {
        mint_lp_tokens_for_market(
            market_account_signer_seeds,
            market_mint_lp_token_accounts,
            expected_lp_tokens_minted,
            token_0_amt,
            token_1_amt,
        )?;
        market_account.token_0_fee_balance_pool.decr_balance(token_0_amt)?;
        market_account.token_1_fee_balance_pool.decr_balance(token_1_amt)?;
        market_account.lp_token_fee_balance_pool.incr_balance(expected_lp_tokens_minted)?;
        Ok((expected_lp_tokens_minted, token_0_amt, token_1_amt))
    } else {
        Ok((0, 0, 0))
    }
}

//
// Position related utils.
//

pub fn close_position_account<'info>(
    position_account_info: &AccountInfo<'info>,
    dest_account_info: &AccountInfo<'info>,
) -> Result<(), LimitlessError> {
    // Clear position data.
    state::clear_data(position_account_info.data.borrow_mut().deref_mut());

    // Transfer remaining balance to destination account.
    let balance = position_account_info.lamports();
    **position_account_info.lamports.borrow_mut() = 0;
    let dest_account_balance = dest_account_info.lamports();
    **dest_account_info.lamports.borrow_mut() = dest_account_balance.safe_add(balance)?;
    Ok(())
}

pub fn rollover_position(
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    position_account: &mut PositionAccount,
    market_account: &MarketAccount,
    limitless_lp_tokens_borrowed: u64,
    limitless_lp_token_supply: u64,
    raydium_quote_token_amt: u64,
    raydium_lp_supply: u64,
    prorated_fee: u64,
    current_slot: u64,
    max_fee: u64,
    duration: u64,

    user_address: Pubkey,
    event_authority_account_info: &AccountInfo,
    event_authority_signer_seeds: &EventAuthoritySignerSeeds,
) -> Result<(u64, u64), LimitlessError> {
    let liquidity_value = calculator::liquidity_value_for_borrow(
        position_account.lp_tokens_removed,
        raydium_lp_supply,
        raydium_quote_token_amt,
    )?;
    let roll_over_fee_quote_token_amt = calculator::blackwing_fee_quote_token_amt(
        liquidity_value,
        position_account.lp_tokens_removed,
        duration,
        market_account.base_fee_apr,
        limitless_lp_tokens_borrowed,
        limitless_lp_token_supply,
        market_account.min_fee_quote_token,
        true,
    )?;
    if max_fee != 0 && roll_over_fee_quote_token_amt > max_fee {
        log!("MaxRolloverFeeExceeded: needed {}, limit {}", roll_over_fee_quote_token_amt, max_fee);
        return Err(LimitlessError::MaxRolloverFeeExceeded);
    }
    if position_account.rollover_reserve_quote_token_amt < roll_over_fee_quote_token_amt {
        log!("InsufficientReserveBalanceToRollover: needed {}, have {}", roll_over_fee_quote_token_amt, position_account.rollover_reserve_quote_token_amt);
        return Err(LimitlessError::InsufficientReserveBalanceToRollover);
    }
    let fees_to_return = position_account.blackwing_fee_reserve_quote_token_amt
        .safe_sub(prorated_fee)?;
    position_account.blackwing_fee_reserve_quote_token_amt = roll_over_fee_quote_token_amt;
    position_account.rollover_reserve_quote_token_amt = position_account.rollover_reserve_quote_token_amt
        .safe_sub(roll_over_fee_quote_token_amt)?
        .safe_add(fees_to_return)?;
    position_account.close_block = current_slot.safe_add(duration)?;
    position_account.quote_token_balance = position_account.quote_token_balance
        .safe_sub(prorated_fee)?;

    let (base_token_mint, quote_token_mint) = match market_account.quote_token {
        QuoteToken::Token1 => (token_0_mint, token_1_mint),
        QuoteToken::Token0 => (token_1_mint, token_0_mint),
    };
    let rollover_position_event = RolloverPositionEvent{
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        user_address,
        id: position_account.id,
        open_block: position_account.open_block,
        blackwing_fee_reserve_amt: position_account.blackwing_fee_reserve_quote_token_amt,
        rollover_fee_reserve_amt: position_account.rollover_reserve_quote_token_amt,
    };
    let event_cpi_ix = emit_cpi_ix(&rollover_position_event, event_authority_account_info.key);
    invoke_signed(
        &event_cpi_ix,
        &[event_authority_account_info.clone()],
        &[&event_authority_signer_seeds.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    Ok((roll_over_fee_quote_token_amt, fees_to_return))
}

//
// Event helpers.
//

pub fn emit_market_state_update(
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    market_account: &MarketAccount,
    market_state: &MarketState,
    event_authority_account_info: &AccountInfo,
    event_authority_signer_seeds: &EventAuthoritySignerSeeds,
) -> Result<(), LimitlessError> {
    let (
        base_token_mint,
        account_base_token_balance,
        base_token_fees_total_shares,
        base_token_fees_total_balance,
        base_token_fees_total_fake_balance,
        quote_token_mint,
        account_quote_token_balance,
        quote_token_fees_total_shares,
        quote_token_fees_total_balance,
        quote_token_fees_total_fake_balance,
    ) = match market_account.quote_token {
        QuoteToken::Token1 => (
            token_0_mint,
            market_state.token_0_balance(),
            market_account.token_0_fee_balance_pool.share_token_supply(),
            market_account.token_0_fee_balance_pool.total_balance(),
            market_account.token_0_fee_balance_pool.fake_total_balance(),
            token_1_mint,
            market_state.token_1_balance(),
            market_account.token_1_fee_balance_pool.share_token_supply(),
            market_account.token_1_fee_balance_pool.total_balance(),
            market_account.token_1_fee_balance_pool.fake_total_balance(),
        ),
        QuoteToken::Token0 => (
            token_1_mint,
            market_state.token_1_balance(),
            market_account.token_1_fee_balance_pool.share_token_supply(),
            market_account.token_1_fee_balance_pool.total_balance(),
            market_account.token_1_fee_balance_pool.fake_total_balance(),
            token_0_mint,
            market_state.token_0_balance(),
            market_account.token_0_fee_balance_pool.share_token_supply(),
            market_account.token_0_fee_balance_pool.total_balance(),
            market_account.token_0_fee_balance_pool.fake_total_balance(),
        ),
    };
    let event = MarketStateUpdateEvent {
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        account_base_token_balance,
        account_quote_token_balance,
        account_lp_token_balance: market_state.lp_token_balance(),

        lp_tokens_removed_for_positions: market_account.lp_tokens_removed_for_positions,
        lp_tokens_supplied_total_shares: market_account.lp_tokens_supplied_pool.share_token_supply(),
        lp_tokens_supplied_total_balance: market_account.lp_tokens_supplied_pool.total_balance(),
        base_token_fees_total_shares,
        base_token_fees_total_balance,
        base_token_fees_total_fake_balance,
        quote_token_fees_total_shares,
        quote_token_fees_total_balance,
        quote_token_fees_total_fake_balance,
        lp_token_fees_total_shares: market_account.lp_token_fee_balance_pool.share_token_supply(),
        lp_token_fees_total_balance: market_account.lp_token_fee_balance_pool.total_balance(),
        lp_token_fees_total_fake_balance: market_account.lp_token_fee_balance_pool.fake_total_balance(),
    };
    let event_cpi_ix = emit_cpi_ix(&event, event_authority_account_info.key);
    invoke_signed(
        &event_cpi_ix,
        &[event_authority_account_info.clone()],
        &[&event_authority_signer_seeds.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;
    Ok(())
}

//
// Helpers
//

struct SwapTokenAccounts<'a, 'b: 'a> {
    token_to_swap_mint: &'a AccountInfo<'b>,
    token_to_swap_source_acc: &'a AccountInfo<'b>,
    token_to_swap_pool_vault: &'a AccountInfo<'b>,
    token_to_receive_mint: &'a AccountInfo<'b>,
    token_to_receive_dest_acc: &'a AccountInfo<'b>,
    token_to_receive_pool_vault: &'a AccountInfo<'b>,
}

fn get_swap_token_accounts<'a, 'b: 'a>(
    token_to_swap_mint: &Pubkey,
    token_0_mint: &'a AccountInfo<'b>,
    token_1_mint: &'a AccountInfo<'b>,
    token_0_vault: &'a AccountInfo<'b>,
    token_1_vault: &'a AccountInfo<'b>,
    swap_source_token_0_acc: &'a AccountInfo<'b>,
    swap_source_token_1_acc: &'a AccountInfo<'b>,
) -> SwapTokenAccounts<'a, 'b> {
    if token_to_swap_mint.eq(token_1_mint.key) {
        SwapTokenAccounts {
            token_to_swap_mint: token_1_mint,
            token_to_swap_source_acc: swap_source_token_1_acc,
            token_to_swap_pool_vault: token_1_vault,
            token_to_receive_mint: token_0_mint,
            token_to_receive_dest_acc: swap_source_token_0_acc,
            token_to_receive_pool_vault: token_0_vault,
        }
    } else {
        SwapTokenAccounts {
            token_to_swap_mint: token_0_mint,
            token_to_swap_source_acc: swap_source_token_0_acc,
            token_to_swap_pool_vault: token_0_vault,
            token_to_receive_mint: token_1_mint,
            token_to_receive_dest_acc: swap_source_token_1_acc,
            token_to_receive_pool_vault: token_1_vault,
        }
    }
}