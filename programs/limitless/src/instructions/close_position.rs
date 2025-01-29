use std::ops::Deref;
use anchor_lang::context::CpiContext;
use anchor_lang::Key;
use anchor_spl::token_2022::spl_token_2022;
use solana_program::account_info::AccountInfo;
use solana_program::program::invoke_signed;
use uuid::Uuid;
use blackwing_proc_macros::{account_infos_struct, ToAccountMetaList};
use utils::{log, numbers::SafeUnsigned, raydium::raydium_cp_swap};
use utils::events::EventAuthoritySignerSeeds;
use utils::instructions::{ToAccountMetaList, AccountKey, WriteableAccountKey, DynamicAccountKey, SignerAccountKey};
use utils::state::Packable;
use utils::validate::validate_signer;
use crate::errors::LimitlessError;
use crate::state::position::PositionAccount;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, MarketIntermediateTokenAccountSignerSeeds, MarketState, MarketStateAccounts, QuoteToken};
use crate::calculator::{self, BLACKWING_FEE_DENOM, BLACKWING_PROTOCOL_FEE};
use crate::events::{emit_cpi_ix, PositionCloseEvent};
use crate::raydium::state::{RaydiumState, RaydiumStateAccounts};
use crate::instructions::utils::{MarketSwapAccounts, market_swap_exact_input, transfer_spl_from_market_to_user, close_position_account, rollover_position, deposit_fees_as_liquidity, MarketMintLpTokenAccounts, emit_market_state_update, validate_fee_collector_ata, transfer_spl_from_market_to_user_unwrap_wsol};
use crate::state::config::{ConfigAccount, TradingMode};
use crate::state::is_data_cleared;

#[account_infos_struct(ClosePositionAccountsInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct ClosePositionAccounts {
    pub user_account: DynamicAccountKey,
    pub payer_account: SignerAccountKey,
    pub market_account: WriteableAccountKey,
    pub position_account: WriteableAccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub market_token_0_ata: WriteableAccountKey,
    pub market_token_1_ata: WriteableAccountKey,
    pub market_intermediate_wsol_ta: WriteableAccountKey,
    pub raydium_lp_mint: WriteableAccountKey,
    pub market_raydium_lp_token_ata: WriteableAccountKey,
    pub user_token_0_ata: WriteableAccountKey,
    pub user_token_1_ata: WriteableAccountKey,
    pub fee_collector_quote_token_ata: WriteableAccountKey,
    pub raydium_program: AccountKey,
    pub raydium_config: AccountKey,
    pub pool_state: WriteableAccountKey,
    pub pool_authority: AccountKey,
    pub pool_observation: WriteableAccountKey,
    pub token_0_vault: WriteableAccountKey,
    pub token_1_vault: WriteableAccountKey,
    pub token_program: AccountKey,
    pub token_2022_program: AccountKey,
    pub associated_token_account_program: AccountKey,
    pub memo_program: AccountKey,
    pub system_program: AccountKey,
    pub rent: AccountKey,
    pub limitless_program: AccountKey,
    pub event_authority: AccountKey,
    pub limitless_config: AccountKey,
}

impl<'refs, 'info> ClosePositionAccountsInfos<'refs, 'info> {
    fn market_state_accounts(&'refs self) -> MarketStateAccounts<'refs, 'info> {
        MarketStateAccounts {
            market_account: self.market_account,
            token_0_ata: self.market_token_0_ata,
            token_1_ata: self.market_token_1_ata,
            raydium_lp_ata: self.market_raydium_lp_token_ata,
        }
    }

    fn raydium_state_accounts(&'refs self) -> RaydiumStateAccounts<'refs, 'info> {
        RaydiumStateAccounts {
            amm_config: self.raydium_config,
            pool_state: self.pool_state,
            token_0_vault: self.token_0_vault,
            token_1_vault: self.token_1_vault,
        }
    }

    fn market_mint_lp_token_accounts(&'refs self) -> MarketMintLpTokenAccounts<'refs, 'info> {
        MarketMintLpTokenAccounts {
            market_account: self.market_account,
            market_token_0_ata: self.market_token_0_ata,
            market_token_1_ata: self.market_token_1_ata,
            market_raydium_lp_token_ata: self.market_raydium_lp_token_ata,
            token_0_mint: self.token_0_mint,
            token_1_mint: self.token_1_mint,
            raydium_lp_mint: self.raydium_lp_mint,
            token_0_vault: self.token_0_vault,
            token_1_vault: self.token_1_vault,
            token_program: self.token_program,
            token_program2022: self.token_2022_program,
            raydium_program: self.raydium_program,
            pool_state: self.pool_state,
            pool_authority: self.pool_authority,
        }
    }

    fn market_swap_accounts(&'refs self) -> MarketSwapAccounts<'refs, 'info> {
        MarketSwapAccounts {
            market_account: self.market_account,
            market_token_0_ata: self.market_token_0_ata,
            market_token_1_ata: self.market_token_1_ata,
            token_0_mint: self.token_0_mint,
            token_1_mint: self.token_1_mint,
            token_0_vault: self.token_0_vault,
            token_1_vault: self.token_1_vault,
            token_program: self.token_program,
            raydium_program: self.raydium_program,
            raydium_config: self.raydium_config,
            pool_state: self.pool_state,
            pool_authority: self.pool_authority,
            pool_observation: self.pool_observation,

        }
    }
}

struct SignerSeeds {
    market_account: MarketAccountSignerSeeds,
    market_intermediate_wsol_ta: MarketIntermediateTokenAccountSignerSeeds,
    event_authority: EventAuthoritySignerSeeds,
}

pub fn process_close_position(
    accounts: &[AccountInfo],
    position_id: Uuid,
    worst_price_num: u64,
    worst_price_den: u64,
) -> Result<(), LimitlessError> {
    if worst_price_den == 0 {
        log!("InvalidInstruction: worst_price_den cannot be 0");
        return Err(LimitlessError::InvalidInstruction);
    }

    log!("Processing position close: id {}", position_id);

    let (
        account_infos,
        seeds,
    ) = parse_accounts(accounts, &position_id)?;

    let config_account = ConfigAccount::unpack(account_infos.limitless_config)
        .map_err(|_| LimitlessError::ConfigAccountSerializationFailed)?;
    let mut market_account = MarketAccount::unpack(&account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;
    let mut market_state = MarketState::load(account_infos.market_state_accounts())?;
    let mut raydium_state = RaydiumState::load(account_infos.raydium_state_accounts())?;
    if is_data_cleared(account_infos.position_account.data.borrow().deref()) {
        return Err(LimitlessError::PositionDoesNotExist);
    }
    let mut position_account = PositionAccount::unpack(account_infos.position_account)
        .map_err(|_| LimitlessError::PositionStateSerializationFailed)?;

    if config_account.trading_mode != TradingMode::Enabled && config_account.trading_mode != TradingMode::PositionCloseOnly {
        return Err(LimitlessError::MarketClosed);
    }
    if market_account.trading_mode != TradingMode::Enabled && market_account.trading_mode != TradingMode::PositionCloseOnly {
        return Err(LimitlessError::MarketClosed);
    }

    validate_fee_collector_ata(
        &account_infos.fee_collector_quote_token_ata,
        &market_account,
        &config_account,
        &account_infos.token_0_mint,
        &account_infos.token_1_mint,
        &account_infos.token_program,
    )?;

    let (base_token_mint, quote_token_mint) = market_account.get_base_and_quote_keys(
        account_infos.token_0_mint.key,
        account_infos.token_1_mint.key,
    );

    let (
        quote_market_ata_info,
        quote_user_ata_info,
        quote_mint,
    ) = if base_token_mint == account_infos.token_0_mint.key {
        (
            account_infos.market_token_1_ata,
            account_infos.user_token_1_ata,
            account_infos.token_1_mint,
        )
    } else {
        (
            account_infos.market_token_0_ata,
            account_infos.user_token_0_ata,
            account_infos.token_0_mint,
        )
    };


    let base_token_pool_amt = raydium_state.base_token_amt(&market_account);
    let quote_token_pool_amt = raydium_state.quote_token_amt(&market_account);
    let (lp_tokens_minted, _token_0_deposited, _token_1_deposited) = if !position_account.is_short {
        deposit_fees_as_liquidity(
            &mut market_account,
            base_token_pool_amt,
            quote_token_pool_amt,
            raydium_state.lp_supply(),
            raydium_state.trade_fee_rate(),
            raydium_state.protocol_fee_rate(),
            raydium_state.fund_fee_rate(),
            quote_token_mint,
            &seeds.market_account,
            &account_infos.market_swap_accounts(),
            &account_infos.market_mint_lp_token_accounts(),
        )?
    } else {
        (0, 0, 0)
    };
    if lp_tokens_minted > 0 {
        log!("Deposited fees as liquidity");
        log!("{} LP tokens received, {} token_0 deposited, {} token_1 deposited", lp_tokens_minted, _token_0_deposited, _token_1_deposited);
        market_state.reload(account_infos.market_state_accounts())?;
        raydium_state.reload(account_infos.raydium_state_accounts())?;
    }

    let open_base_token_amt = raydium_state.base_token_amt(&market_account);
    let open_quote_token_amt = raydium_state.quote_token_amt(&market_account);

    let position_size_swap_fee: u64 = utils::raydium::fees::trading_fee(
        position_account.position_size as u128,
        raydium_state.trade_fee_rate(),
    )?.try_into()?;
    let total_position_tokens = position_account.position_size
        .safe_sub(position_size_swap_fee)?;
    let total_collateral_tokens = position_account.collateral_amt
        .safe_add(position_account.rounding_fee_reserve_collateral_token_amt)?;

    let market_base_token_balance_before = market_state.base_token_balance();
    let market_quote_token_balance_before = market_state.quote_token_balance();

    let current_slot = utils::state::clock()?.slot;
    let prorated_fee = calculator::prorated_fee(
        position_account.open_block,
        position_account.close_block.safe_sub(position_account.open_block)?,
        current_slot,
        position_account.blackwing_fee_reserve_quote_token_amt,
        market_account.min_fee_quote_token,
    )?;
    let blackwing_earned_fee_quote_tokens: u64 = (prorated_fee as u128)
        .safe_mul(BLACKWING_PROTOCOL_FEE)?
        .safe_div(BLACKWING_FEE_DENOM)?
        .try_into()?;
    let lp_earned_fee_quote_tokens = prorated_fee.safe_sub(blackwing_earned_fee_quote_tokens)?;
    log!(
        "Blackwing Fees: prorated fees charged: {}, lp portion: {}, protocol portion: {}",
        prorated_fee, lp_earned_fee_quote_tokens, blackwing_earned_fee_quote_tokens,
    );

    transfer_spl_from_market_to_user(
        &seeds.market_account,
        account_infos.market_account,
        quote_market_ata_info,
        account_infos.fee_collector_quote_token_ata,
        account_infos.token_program,
        blackwing_earned_fee_quote_tokens,
    )?;
    match market_account.quote_token {
        QuoteToken::Token1 => {
            market_account.token_1_fee_balance_pool.incr_balance(lp_earned_fee_quote_tokens)?;
        }
        QuoteToken::Token0 => {
            market_account.token_0_fee_balance_pool.incr_balance(lp_earned_fee_quote_tokens)?;
        }
    }

    let is_position_duration_expired = current_slot >= position_account.close_block;
    if !is_position_duration_expired {
        // Only if the position is expired, do we allow other accounts to close it.
        // Otherwise, only the owner is allowed to close the position.
        validate_signer(account_infos.user_account, LimitlessError::InvalidSigner)?;
    } else if !account_infos.user_account.is_signer {
        // can only be closer by the closer or admin
        if !account_infos.payer_account.key.eq(&crate::closer::ID) {
            return Err(LimitlessError::InvalidCloser);
        }

        // If another account is trying to close the position, check to see if there is enough fee
        // balance to rollover the position and do that instead.
        let max_fee = position_account.rollover_max_fee_quote_token_amt;
        let duration = position_account.rollover_duration_blocks;
        match rollover_position(
            account_infos.token_0_mint.key,
            account_infos.token_1_mint.key,
            &mut position_account,
            &market_account,
            market_account.lp_tokens_removed_for_positions,
            market_account.lp_tokens_supplied_pool.total_balance(),
            raydium_state.quote_token_amt(&market_account),
            raydium_state.lp_supply(),
            prorated_fee,
            current_slot,
            max_fee,
            duration,
            account_infos.user_account.key(),
            account_infos.event_authority,
            &seeds.event_authority,
        ) {
            Ok((_fees, _fees_returned)) => {
                log!("Rolled over position instead of closing it, charged {} quote token, returned {} quote token", _fees, _fees_returned);
                market_account.pack(account_infos.market_account)
                    .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;
                position_account.pack(account_infos.position_account)
                    .map_err(|_| LimitlessError::PositionAccountSerializationFailed)?;
                return Ok(());
            }
            Err(_e) => {
                log!("Couldn't rollover position ({}), continuing with close", _e);
                // If there was an error, the above function should have made no changes.
            }
        }
    }

    let calcs = calculator::close_position_calcs(
        position_account.loan_position_token_amt,
        position_account.loan_collateral_token_amt,
        total_position_tokens,
        total_collateral_tokens,
        position_account.is_short,
        if !position_account.is_short {
            raydium_state.base_token_amt(&market_account)
        } else {
            raydium_state.quote_token_amt(&market_account)
        },
        if !position_account.is_short {
            raydium_state.quote_token_amt(&market_account)
        } else {
            raydium_state.base_token_amt(&market_account)
        },
        raydium_state.lp_supply(),
        raydium_state.trade_fee_rate(),
        raydium_state.protocol_fee_rate(),
        raydium_state.fund_fee_rate(),
        if !position_account.is_short {
            position_account.base_token_balance
        } else {
            position_account.quote_token_balance
                .safe_sub(position_account.blackwing_fee_reserve_quote_token_amt)?
        },
        if !position_account.is_short {
            position_account.quote_token_balance
                .safe_sub(position_account.blackwing_fee_reserve_quote_token_amt)?
        } else {
            position_account.base_token_balance
        },
    )?;
    log!("Calculation results: {:?}", calcs);

    let remaining_base_token_balance = if !position_account.is_short {
        position_account.base_token_balance
            .safe_sub(calcs.position_tokens_to_swap)?
            .safe_add(calcs.expected_position_tokens_received)?
            .safe_sub(calcs.position_tokens_returned_as_fee)?
            .safe_sub(calcs.position_tokens_to_deposit)?
    } else {
        position_account.base_token_balance
            .safe_sub(calcs.collateral_tokens_to_swap)?
            .safe_add(calcs.expected_collateral_tokens_received)?
            .safe_sub(calcs.collateral_tokens_returned_as_fee)?
            .safe_sub(calcs.collateral_tokens_to_deposit)?
    };
    let remaining_quote_token_balance = if !position_account.is_short {
        position_account.quote_token_balance
            .safe_sub(calcs.collateral_tokens_to_swap)?
            .safe_add(calcs.expected_collateral_tokens_received)?
            .safe_sub(calcs.collateral_tokens_returned_as_fee)?
            .safe_sub(calcs.collateral_tokens_to_deposit)?
            .safe_sub(prorated_fee)?
    } else {
        position_account.quote_token_balance
            .safe_sub(calcs.position_tokens_to_swap)?
            .safe_add(calcs.expected_position_tokens_received)?
            .safe_sub(calcs.position_tokens_to_deposit)?
            .safe_sub(calcs.position_tokens_returned_as_fee)?
            .safe_sub(prorated_fee)?
    };

    let (base_tokens_returned_as_fee, quote_tokens_returned_as_fee) = if !position_account.is_short {
        (calcs.position_tokens_returned_as_fee, calcs.collateral_tokens_returned_as_fee)
    } else {
        (calcs.collateral_tokens_returned_as_fee, calcs.position_tokens_returned_as_fee)
    };
    let expected_market_base_token_balance_at_end = market_base_token_balance_before
        .safe_sub(position_account.base_token_balance)?
        .safe_add(base_tokens_returned_as_fee)?;
    let expected_market_quote_token_balance_at_end = market_quote_token_balance_before
        .safe_sub(position_account.quote_token_balance)?
        .safe_add(lp_earned_fee_quote_tokens)?
        .safe_add(quote_tokens_returned_as_fee)?;

    // Perform swaps.
    let collateral_tokens_received = if calcs.position_tokens_to_swap > 0 {
        if !position_account.is_short {
            market_swap_exact_input(
                &seeds.market_account,
                &account_infos.market_swap_accounts(),
                base_token_mint,
                calcs.position_tokens_to_swap,
                0,
            )?
        } else {
            market_swap_exact_input(
                &seeds.market_account,
                &account_infos.market_swap_accounts(),
                quote_token_mint,
                calcs.position_tokens_to_swap,
                0,
            )?
        }
    } else {
        0
    };
    if collateral_tokens_received != calcs.expected_collateral_tokens_received {
        log!(
            "InvalidCollateralTokensReceivedOnClosePosition: expected {} actual {}",
            calcs.expected_collateral_tokens_received, collateral_tokens_received,
        );
        return Err(LimitlessError::InvalidCollateralTokenOutput);
    }
    let position_tokens_received = if calcs.collateral_tokens_to_swap > 0 {
        if !position_account.is_short {
            market_swap_exact_input(
                &seeds.market_account,
                &account_infos.market_swap_accounts(),
                quote_token_mint,
                calcs.collateral_tokens_to_swap,
                0,
            )?
        } else {
            market_swap_exact_input(
                &seeds.market_account,
                &account_infos.market_swap_accounts(),
                base_token_mint,
                calcs.collateral_tokens_to_swap,
                0,
            )?
        }
    } else {
        0
    };
    if position_tokens_received != calcs.expected_position_tokens_received {
        log!(
            "InvalidPositionTokensReceivedOnClosePosition: expected {} actual {}",
            calcs.expected_position_tokens_received, position_tokens_received,
        );
        return Err(LimitlessError::InvalidPositionTokenOutput);
    }

    // Deposit tokens into the pool.
    let (actual_base_token_deposited, actual_quote_token_deposited) = if calcs.lp_tokens_minted > 0 {
         let (token_0_deposited, token_1_deposited) = deposit_liquidity(
            &seeds.market_account,
            &mut market_state,
            &account_infos,
            calcs.lp_tokens_minted,
            if !position_account.is_short {
                match market_account.quote_token {
                    QuoteToken::Token1 => calcs.position_tokens_to_deposit,
                    QuoteToken::Token0 => calcs.collateral_tokens_to_deposit,
                }
            } else {
                match market_account.quote_token {
                    QuoteToken::Token1 => calcs.collateral_tokens_to_deposit,
                    QuoteToken::Token0 => calcs.position_tokens_to_deposit,
                }
            },
            if !position_account.is_short {
                match market_account.quote_token {
                    QuoteToken::Token1 => calcs.collateral_tokens_to_deposit,
                    QuoteToken::Token0 => calcs.position_tokens_to_deposit,
                }
            } else {
                match market_account.quote_token {
                    QuoteToken::Token1 => calcs.position_tokens_to_deposit,
                    QuoteToken::Token0 => calcs.collateral_tokens_to_deposit,
                }
            },
        )?;
        match market_account.quote_token {
            QuoteToken::Token1 => (token_0_deposited, token_1_deposited),
            QuoteToken::Token0 => (token_1_deposited, token_0_deposited),
        }
    } else {
        (0, 0)
    };
    if !position_account.is_short {
        if actual_base_token_deposited != calcs.position_tokens_to_deposit {
            log!(
                "ClosePositionInvalidPositionTokenInput: expected {} actual {}",
                calcs.position_tokens_to_deposit, actual_base_token_deposited,
            );
            return Err(LimitlessError::InvalidPositionTokenInput);
        }
        if actual_quote_token_deposited != calcs.collateral_tokens_to_deposit {
            log!(
                "InvalidCollateralTokenInput: expected {} actual {}",
                calcs.collateral_tokens_to_deposit, actual_quote_token_deposited,
            );
            return Err(LimitlessError::InvalidCollateralTokenInput);
        }
    } else {
        if actual_base_token_deposited != calcs.collateral_tokens_to_deposit {
            log!(
                "InvalidCollateralTokenInput: expected {} actual {}",
                calcs.collateral_tokens_to_deposit, actual_base_token_deposited,
            );
            return Err(LimitlessError::InvalidCollateralTokenInput);
        }
        if actual_quote_token_deposited != calcs.position_tokens_to_deposit {
            log!(
                "ClosePositionInvalidPositionTokenInput: expected {} actual {}",
                calcs.position_tokens_to_deposit, actual_quote_token_deposited,
            );
            return Err(LimitlessError::InvalidPositionTokenInput);
        }
    }

    market_account.lp_tokens_supplied_pool.decr_balance(position_account.lp_tokens_removed)?;
    market_account.lp_tokens_supplied_pool.incr_balance(calcs.lp_tokens_minted)?;
    market_account.lp_tokens_removed_for_positions = market_account.lp_tokens_removed_for_positions
        .safe_sub(position_account.lp_tokens_removed)?;
    log!("Deposited {} token_0 and {} token_1 into the pool", actual_base_token_deposited, actual_quote_token_deposited);
    log!("Received {} LP tokens", calcs.lp_tokens_minted);

    market_account.token_0_fee_balance_pool.incr_balance(
        match market_account.quote_token {
            QuoteToken::Token1 => base_tokens_returned_as_fee,
            QuoteToken::Token0 => quote_tokens_returned_as_fee,
        }
    )?;
    market_account.token_1_fee_balance_pool.incr_balance(
        match market_account.quote_token {
            QuoteToken::Token1 => quote_tokens_returned_as_fee,
            QuoteToken::Token0 => base_tokens_returned_as_fee,
        }
    )?;
    log!("Retained {} token_0 and {} token_1 as fees", base_tokens_returned_as_fee, quote_tokens_returned_as_fee);

    // Swap remaining base_token to quote_token and return to the user.
    let (quote_tokens_received_from_base_token_swap, base_tokens_burned, base_tokens_swapped) = if remaining_base_token_balance > 0 {
        // Determine if swapping the remaining base token balance will yield any quote tokens.
        raydium_state.reload(account_infos.raydium_state_accounts())?;
        let (output, _, _) = calculator::calculate_swap_result_u128(
            remaining_base_token_balance as u128,
            raydium_state.base_token_amt(&market_account) as u128,
            raydium_state.quote_token_amt(&market_account) as u128,
            raydium_state.trade_fee_rate(),
            raydium_state.protocol_fee_rate(),
            raydium_state.fund_fee_rate(),
        )?;
        if output > 0 {
            (
                market_swap_exact_input(
                    &seeds.market_account,
                    &account_infos.market_swap_accounts(),
                    base_token_mint,
                    remaining_base_token_balance,
                    output as u64,
                )?,
                0,
                remaining_base_token_balance,
            )
        } else {
            (0, remaining_base_token_balance, 0)
        }
    } else {
        (0, 0, 0)
    };
    log!(
        "Swapped {} base_tokens for {} quote_tokens. burned {} base_tokens",
        base_tokens_swapped,
        quote_tokens_received_from_base_token_swap,
        base_tokens_burned,
    );

    // Reload raydium state.
    raydium_state.reload(account_infos.raydium_state_accounts())?;
    let after_base_token_amt = raydium_state.base_token_amt(&market_account);
    let after_quote_token_amt = raydium_state.quote_token_amt(&market_account);

    // Check slippage. If position is expired, we don't check slippage.
    if !is_position_duration_expired {
        calculator::check_slippage_closing(
            worst_price_num,
            worst_price_den,
            after_base_token_amt,
            after_quote_token_amt,
            position_account.is_short,
        )?;
    } else {
        log!("Not checking slippage because position duration is expired");
    }

    // Transfer all token_1 to user.
    let transfer_amt = remaining_quote_token_balance.safe_add(quote_tokens_received_from_base_token_swap)?;
    transfer_spl_from_market_to_user_unwrap_wsol(
        &seeds.market_account,
        &seeds.market_intermediate_wsol_ta,
        account_infos.market_account,
        quote_market_ata_info,
        quote_user_ata_info,
        account_infos.user_account,
        account_infos.payer_account,
        quote_mint,
        account_infos.market_intermediate_wsol_ta,
        account_infos.token_program,
        account_infos.system_program,
        account_infos.rent,
        transfer_amt,
    )?;

    log!(
        "Quote: start_quote_token_amt: {}, start_base_token_amt: {}, end_quote_token_amt: {}, end_base_token_amt: {}, \
quote_token_transferred_to_user: {}, amm_swap_fees_quote_token: {}, prorated_blackwing_fee: {}, collateral_quote_token: {}, \
blackwing_fee_reserve_quote_token: {}, amm_fee_reserve_quote_token: {}",
        open_quote_token_amt,
        open_base_token_amt,
        after_quote_token_amt,
        after_base_token_amt,
        transfer_amt,
        calcs.amm_fees_charged_quote_token,
        prorated_fee,
        position_account.user_quote_token_collateral_amt,
        position_account.blackwing_fee_reserve_quote_token_amt,
        position_account.raydium_fees_reserved_amt_quote_token,
    );

    market_state.reload(account_infos.market_state_accounts())?;
    if market_state.base_token_balance() != expected_market_base_token_balance_at_end.safe_add(base_tokens_burned)? {
        log!(
            "InvalidBaseTokenBalanceAtEnd: expected {} actual {}",
            expected_market_base_token_balance_at_end, market_state.base_token_balance(),
        );
        return Err(LimitlessError::InvalidBaseTokenBalanceAtEnd);
    }
    if market_state.quote_token_balance() != expected_market_quote_token_balance_at_end {
        log!(
            "InvalidQuoteTokenBalanceAtEnd: expected {} actual {}",
            expected_market_quote_token_balance_at_end, market_state.quote_token_balance(),
        );
        return Err(LimitlessError::InvalidQuoteTokenBalanceAtEnd);
    }

    // Update market.
    market_account.pack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;
    emit_market_state_update(
        account_infos.token_0_mint.key,
        account_infos.token_1_mint.key,
        &market_account,
        &market_state,
        &account_infos.event_authority,
        &seeds.event_authority,
    )?;

    // Close position.
    close_position_account(
        account_infos.position_account,
        account_infos.user_account,
    )?;

    let position_close_event = PositionCloseEvent{
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        user_address: account_infos.user_account.key(),
        id: position_id,
        open_block: position_account.open_block,
        blackwing_fees_charged: prorated_fee,
        amt_transferred_to_user: transfer_amt,
        close_x: open_base_token_amt,
        close_y: open_quote_token_amt,
        after_close_x: after_base_token_amt,
        after_close_y: after_quote_token_amt,
        raydium_fee_charged_amt_quote_token: calcs.amm_fees_charged_quote_token,
    };
    let event_cpi_ix = emit_cpi_ix(&position_close_event, account_infos.event_authority.key);
    invoke_signed(
        &event_cpi_ix,
        &[account_infos.event_authority.clone()],
        &[&seeds.event_authority.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    Ok(())
}

fn deposit_liquidity(
    market_account_signer_seeds: &MarketAccountSignerSeeds,
    market_state: &mut MarketState,
    account_infos: &ClosePositionAccountsInfos,
    lp_tokens_to_receive: u64,
    token_0_to_deposit: u64,
    token_1_to_deposit: u64,
) -> Result<(u64, u64), LimitlessError> {
    market_state.reload(account_infos.market_state_accounts())?;
    let before_raydium_lp_balance = market_state.lp_token_balance();
    let before_token_0_market_balance = market_state.token_0_balance();
    let before_token_1_market_balance = market_state.token_1_balance();

    let deposit_instruction = raydium_cp_swap::cpi::accounts::Deposit{
        owner: account_infos.market_account.clone(),
        authority: account_infos.pool_authority.clone(),
        pool_state: account_infos.pool_state.clone(),
        owner_lp_token: account_infos.market_raydium_lp_token_ata.clone(),
        token0_account: account_infos.market_token_0_ata.clone(),
        token1_account: account_infos.market_token_1_ata.clone(),
        token0_vault: account_infos.token_0_vault.clone(),
        token1_vault: account_infos.token_1_vault.clone(),
        token_program: account_infos.token_program.clone(),
        token_program2022: account_infos.token_2022_program.clone(),
        vault0_mint: account_infos.token_0_mint.clone(),
        vault1_mint: account_infos.token_1_mint.clone(),
        lp_mint: account_infos.raydium_lp_mint.clone(),
    };
    raydium_cp_swap::cpi::deposit(
        CpiContext::new_with_signer(
            account_infos.raydium_program.clone(),
            deposit_instruction,
            &[&market_account_signer_seeds.as_refs()]
        ),
        lp_tokens_to_receive,
        token_0_to_deposit,
        token_1_to_deposit,
    ).map_err(|_| LimitlessError::DepositLpInvokeFailed )?;

    market_state.reload(account_infos.market_state_accounts())?;
    let lp_balance_check = before_raydium_lp_balance
        .safe_add(lp_tokens_to_receive)?;
    if lp_balance_check != market_state.lp_token_balance() {
        return Err(LimitlessError::InvalidLpTokensReceived);
    };
    Ok((
        before_token_0_market_balance.safe_sub(market_state.token_0_balance())?,
        before_token_1_market_balance.safe_sub(market_state.token_1_balance())?,
    ))
}

fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
    position_id: &Uuid,
) -> Result<(
    ClosePositionAccountsInfos<'slice, 'info>,
    SignerSeeds,
), LimitlessError> {
    let account_info_iter = &mut accounts.iter();

    // User account.
    let user_account = utils::next_account_info(account_info_iter)?;

    // Payer account
    let payer_account = utils::next_account_info(account_info_iter)?;

    // Market account.
    let market_account_info = utils::next_account_info(account_info_iter)?;
    // Position account.
    let position_account_info = utils::next_account_info(account_info_iter)?;
    // Token information.
    let token_0_mint_info = utils::next_account_info(account_info_iter)?;
    let token_1_mint_info = utils::next_account_info(account_info_iter)?;
    let market_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let market_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let market_intermediate_wsol_ta_info = utils::next_account_info(account_info_iter)?;
    let raydium_lp_mint_info = utils::next_account_info(account_info_iter)?;
    let market_raydium_lp_token_ata_info = utils::next_account_info(account_info_iter)?;
    let user_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let user_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let fee_collector_quote_token_ata_info = utils::next_account_info(account_info_iter)?;
    // Raydium specific accounts.
    let raydium_program_info = utils::next_account_info(account_info_iter)?;
    let raydium_config_info = utils::next_account_info(account_info_iter)?;
    let pool_state_info = utils::next_account_info(account_info_iter)?;
    let pool_authority_info = utils::next_account_info(account_info_iter)?;
    let pool_observation_info = utils::next_account_info(account_info_iter)?;
    let token_0_vault_info = utils::next_account_info(account_info_iter)?;
    let token_1_vault_info = utils::next_account_info(account_info_iter)?;
    // Program and system accounts.
    let token_program_info = utils::next_account_info(account_info_iter)?;
    let token_2022_program_info = utils::next_account_info(account_info_iter)?;
    let associated_token_account_program_info = utils::next_account_info(account_info_iter)?;
    let memo_program_info = utils::next_account_info(account_info_iter)?;
    let system_program_info = utils::next_account_info(account_info_iter)?;
    let rent_info = utils::next_account_info(account_info_iter)?;
    let limitless_program_info = utils::next_account_info(account_info_iter)?;
    // Event accounts.
    let event_authority_info = utils::next_account_info(account_info_iter)?;
    // Limitless config.
    let limitless_config_info = utils::next_account_info(account_info_iter)?;

    //
    // Validations.
    //

    // We'll validate signer above because another user can close the position.
    // fee_collector_quote_token_ata_info is validated above.

    utils::validate::validate_account(token_program_info, &spl_token::ID, LimitlessError::InvalidTokenProgramAccount)?;
    utils::validate::validate_account(token_2022_program_info, &spl_token_2022::ID, LimitlessError::InvalidToken2022ProgramAccount)?;
    utils::validate::validate_account(associated_token_account_program_info, &spl_associated_token_account::ID, LimitlessError::InvalidAssociatedTokenProgramAccount)?;
    utils::validate::validate_account(memo_program_info, &spl_memo::ID, LimitlessError::InvalidSplMemoProgramAccount)?;
    utils::validate::validate_account(system_program_info, &solana_program::system_program::ID, LimitlessError::InvalidSystemProgramAccount)?;
    utils::validate::validate_account(rent_info, &solana_program::sysvar::rent::ID, LimitlessError::InvalidRentAccount)?;
    utils::validate::validate_account(limitless_program_info, &crate::ID, LimitlessError::InvalidProgramAccount)?;

    utils::validate::validate_token_mint(token_0_mint_info, token_program_info, LimitlessError::InvalidToken0MintAccount)?;
    utils::validate::validate_token_mint(token_1_mint_info, token_program_info, LimitlessError::InvalidToken1MintAccount)?;
    utils::validate::validate_ata(
        user_token_0_ata_info,
        &token_0_mint_info,
        token_program_info,
        &user_account.key,
        LimitlessError::InvalidUserToken0Ata,
    )?;
    utils::validate::validate_ata(
        user_token_1_ata_info,
        &token_1_mint_info,
        token_program_info,
        &user_account.key,
        LimitlessError::InvalidUserToken1Ata,
    )?;

    let config_account_signer_seed = ConfigAccount::signer_seeds()?;
    utils::validate::validate_pda(
        limitless_config_info,
        &crate::ID,
        &crate::ID,
        &config_account_signer_seed.as_refs(),
        LimitlessError::InvalidLimitlessConfigAccountPda,
    )?;

    let market_account_signer_seeds = MarketAccount::signer_seeds(
        token_0_mint_info.key,
        token_1_mint_info.key,
    )?;
    utils::validate::validate_pda(
        market_account_info,
        &crate::ID,
        &crate::ID,
        &market_account_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketAccountPda,
    )?;
    let market_token_0_account_signer_seeds = MarketAccount::token_account_signer_seeds(
        market_account_info.key,
        token_program_info.key,
        token_0_mint_info.key
    );
    utils::validate::validate_pda(
        market_token_0_ata_info,
        token_program_info.key,
        &crate::ID,
        &market_token_0_account_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketToken0Ata,
    )?;
    let market_token_1_account_signer_seeds = MarketAccount::token_account_signer_seeds(
        market_account_info.key,
        token_program_info.key,
        token_1_mint_info.key
    );
    utils::validate::validate_pda(
        market_token_1_ata_info,
        token_program_info.key,
        &crate::ID,
        &market_token_1_account_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketToken1Ata,
    )?;
    let market_intermediate_wsol_ta_signer_seeds = MarketAccount::intermediate_token_account_signer_seeds(
      market_account_info.key,
      token_program_info.key,
      &spl_token::native_mint::ID,
    );
    utils::validate::validate_pda(
        market_intermediate_wsol_ta_info,
        system_program_info.key, // we expect the account to be uninitialized
        &crate::ID,
        &market_intermediate_wsol_ta_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketIntermediateTa,
    )?;

    let market_raydium_lp_token_account_signer_seeds = MarketAccount::token_account_signer_seeds(
        market_account_info.key,
        token_program_info.key,
        raydium_lp_mint_info.key
    );
    utils::validate::validate_pda(
        market_raydium_lp_token_ata_info,
        token_program_info.key,
        &crate::ID,
        &market_raydium_lp_token_account_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketRaydiumLpTokenAta,
    )?;

    let position_account_signer_seeds = PositionAccount::signer_seeds(
        market_account_info.key,
        user_account.key,
        position_id,
    );
    utils::validate::validate_pda_address(
        position_account_info,
        &crate::ID,
        &position_account_signer_seeds.as_refs(),
        LimitlessError::InvalidPositionAccountPda,
    )?;

    let pool_state = utils::raydium::validate::validate_raydium_accounts_for_pool_state(
        token_0_mint_info,
        token_1_mint_info,
        raydium_program_info,
        raydium_config_info,
        pool_state_info,
        Some(pool_authority_info),
        Some(pool_observation_info),
    )?;
    utils::validate::validate_account(token_0_vault_info, &pool_state.token0_vault, LimitlessError::UtilsErrorInvalidRaydiumToken0VaultAccount)?;
    utils::validate::validate_account(token_1_vault_info, &pool_state.token1_vault, LimitlessError::UtilsErrorInvalidRaydiumToken1VaultAccount)?;
    utils::validate::validate_account(raydium_lp_mint_info, &pool_state.lp_mint, LimitlessError::UtilsErrorInvalidRaydiumLpMintAccount)?;

    let event_authority_seeds = EventAuthoritySignerSeeds::new(&crate::ID);
    utils::validate::validate_pda_address(
        event_authority_info,
        &crate::ID,
        &event_authority_seeds.as_refs(),
        LimitlessError::InvalidEventAuthority,
    )?;

    let accounts = ClosePositionAccountsInfos::<'slice, 'info> {
        user_account,
        payer_account,
        market_account: market_account_info,
        position_account: position_account_info,
        token_0_mint: token_0_mint_info,
        token_1_mint: token_1_mint_info,
        market_token_0_ata: market_token_0_ata_info,
        market_token_1_ata: market_token_1_ata_info,
        market_intermediate_wsol_ta: market_intermediate_wsol_ta_info,
        raydium_lp_mint: raydium_lp_mint_info,
        market_raydium_lp_token_ata: market_raydium_lp_token_ata_info,
        user_token_0_ata: user_token_0_ata_info,
        user_token_1_ata: user_token_1_ata_info,
        fee_collector_quote_token_ata: fee_collector_quote_token_ata_info,
        raydium_program: raydium_program_info,
        raydium_config: raydium_config_info,
        pool_state: pool_state_info,
        pool_authority: pool_authority_info,
        pool_observation: pool_observation_info,
        token_0_vault: token_0_vault_info,
        token_1_vault: token_1_vault_info,
        token_program: token_program_info,
        token_2022_program: token_2022_program_info,
        associated_token_account_program: associated_token_account_program_info.into(),
        memo_program: memo_program_info,
        system_program: system_program_info,
        rent: rent_info,
        limitless_program: limitless_program_info,
        event_authority: event_authority_info,
        limitless_config: limitless_config_info,
    };

    let signer_seeds = SignerSeeds {
        market_account: market_account_signer_seeds,
        market_intermediate_wsol_ta: market_intermediate_wsol_ta_signer_seeds,
        event_authority: event_authority_seeds,
    };

    Ok((accounts, signer_seeds))
}
