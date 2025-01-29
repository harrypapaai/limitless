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
use utils::instructions::{ToAccountMetaList, AccountKey, SignerAccountKey, WriteableAccountKey};
use utils::state::Packable;
use utils::validate::validate_signer;
use crate::errors::LimitlessError;
use crate::state::position::{PositionAccount, PositionAccountSignerSeeds};
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, MarketState, MarketStateAccounts, QuoteToken};
use crate::calculator;
use crate::events::{emit_cpi_ix, PositionOpenEvent};
use crate::raydium::state::{RaydiumState, RaydiumStateAccounts};
use crate::instructions::utils::{UserSwapAccounts, MarketSwapAccounts, market_swap_exact_input, transfer_spl_from_user_to_market, user_swap_exact_input, deposit_fees_as_liquidity, MarketMintLpTokenAccounts, emit_market_state_update, wrap_wsol_if_needed};
use crate::state::config::{ConfigAccount, TradingMode};
use crate::state::is_data_cleared;

#[account_infos_struct(OpenPositionAccountsInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct OpenPositionAccounts {
    pub user_account: SignerAccountKey,
    pub market_account: WriteableAccountKey,
    pub position_account: WriteableAccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub market_token_0_ata: WriteableAccountKey,
    pub market_token_1_ata: WriteableAccountKey,
    pub raydium_lp_mint: WriteableAccountKey,
    pub market_raydium_lp_token_ata: WriteableAccountKey,
    pub user_token_0_ata: WriteableAccountKey,
    pub user_token_1_ata: WriteableAccountKey,
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

impl<'refs, 'info> OpenPositionAccountsInfos<'refs, 'info> {
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

    fn user_swap_accounts(&'refs self) -> UserSwapAccounts<'refs, 'info> {
        UserSwapAccounts {
            user_account: self.user_account,
            user_token_0_ata: self.user_token_0_ata,
            user_token_1_ata: self.user_token_1_ata,
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
    position_account: PositionAccountSignerSeeds,
    event_authority: EventAuthoritySignerSeeds,
}

pub fn process_open_position(
    accounts: &[AccountInfo],
    position_id: Uuid,
    collateral_quote_amt: u64,
    is_short: bool,
    duration_slots: u64,
    max_raydium_fee_amt_quote: u64,
    max_blackwing_fee_amt_quote: u64,
    rollover_reserve_quote_amt: u64,
    worst_price_num: u64,
    worst_price_den: u64,
) -> Result<(), LimitlessError> {
    if worst_price_den == 0 {
        log!("InvalidInstruction: worst_price_den cannot be 0");
        return Err(LimitlessError::InvalidInstruction);
    }

    log!(
        "Processing position open: collateral_quote_amt {} is_short {}",
        collateral_quote_amt, is_short,
    );

    let (
        account_infos,
        seeds,
    ) = parse_accounts(accounts, &position_id)?;

    if !is_data_cleared(account_infos.position_account.data.borrow().deref()) {
        return Err(LimitlessError::PositionIdAlreadyUsed);
    }

    let config_account = ConfigAccount::unpack(account_infos.limitless_config)
        .map_err(|_| LimitlessError::ConfigAccountSerializationFailed)?;
    let mut market_account = MarketAccount::unpack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;
    let mut market_state = MarketState::load(account_infos.market_state_accounts())?;
    let mut raydium_state = RaydiumState::load(account_infos.raydium_state_accounts())?;

    if config_account.trading_mode != TradingMode::Enabled  {
        return Err(LimitlessError::MarketClosed);
    }
    if market_account.trading_mode != TradingMode::Enabled {
        return Err(LimitlessError::MarketClosed);
    }
    if market_account.min_duration_slots > duration_slots {
        log!("InvalidDurationSlots: min {} actual {}", market_account.min_duration_slots, duration_slots);
        return Err(LimitlessError::InvalidDurationSlots);
    }
    if market_account.max_duration_slots < duration_slots {
        log!("InvalidDurationSlots: max {} actual {}", market_account.max_duration_slots, duration_slots);
        return Err(LimitlessError::InvalidDurationSlots);
    }

    let (base_token_mint, quote_token_mint) = market_account.get_base_and_quote_keys(
        account_infos.token_0_mint.key,
        account_infos.token_1_mint.key,
    );
    let (
        base_market_ata_info,
        base_user_ata_info,
        quote_market_ata_info,
        quote_user_ata_info,
    ) = if base_token_mint == account_infos.token_0_mint.key {
        (
            account_infos.market_token_0_ata,
            account_infos.user_token_0_ata,
            account_infos.market_token_1_ata,
            account_infos.user_token_1_ata,
        )
    } else {
        (
            account_infos.market_token_1_ata,
            account_infos.user_token_1_ata,
            account_infos.market_token_0_ata,
            account_infos.user_token_0_ata,
        )
    };


    let base_pool_token_amt = raydium_state.base_token_amt(&market_account);
    let quote_pool_token_amt = raydium_state.quote_token_amt(&market_account);
    let (lp_tokens_minted, _token_0_deposited, _token_1_deposited) = if !is_short {
        // Only run for longs because redepositing fees on shorts results in OOM error.
        deposit_fees_as_liquidity(
            &mut market_account,
            base_pool_token_amt,
            quote_pool_token_amt,
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
        log!("LP tokens minted: {}, token_0 deposited: {}, token_1 deposited: {}", lp_tokens_minted, _token_0_deposited, _token_1_deposited);
        market_state.reload(account_infos.market_state_accounts())?;
        raydium_state.reload(account_infos.raydium_state_accounts())?;
    }

    let open_base_token_amt = raydium_state.base_token_amt(&market_account);
    let open_quote_token_amt = raydium_state.quote_token_amt(&market_account);
    let total_lp_tokens_available = market_account.lp_tokens_supplied_pool.total_balance()
        .safe_sub(market_account.lp_tokens_removed_for_positions)?;

    let (
        collateral_amt,
        max_liquidity,
        position_size,
        loan_position_tokens,
        loan_collateral_tokens,
        pool_position_tokens_after,
        pool_collateral_tokens_after,
        long_calcs,
        short_calcs,
        excess_base_token,
        quote_token_amm_fees,
    )= if !is_short {
        let long_calcs = calculator::open_position_calcs(
            collateral_quote_amt,
            raydium_state.base_token_amt(&market_account),
            raydium_state.quote_token_amt(&market_account),
            raydium_state.lp_supply(),
            total_lp_tokens_available,
            raydium_state.trade_fee_rate(),
            raydium_state.protocol_fee_rate(),
            raydium_state.fund_fee_rate(),
        )?;

        log!("Long: Calculation results: {:?}", long_calcs);

        let quote_token_amm_fees = long_calcs.fees.total_fees;
        (
            collateral_quote_amt,
            long_calcs.max_liquidity,
            long_calcs.expected_position_size,
            long_calcs.expected_position_tokens_removed,
            long_calcs.expected_collateral_tokens_removed,
            long_calcs.expected_pool_position_tokens_after,
            long_calcs.expected_pool_collateral_tokens_after,
            Some(long_calcs),
            None,
            0,
            quote_token_amm_fees
        )
    } else {
        let short_calcs = calculator::open_short_position_calcs(
            collateral_quote_amt,
            raydium_state.base_token_amt(&market_account),
            raydium_state.quote_token_amt(&market_account),
            raydium_state.lp_supply(),
            total_lp_tokens_available,
            raydium_state.trade_fee_rate(),
            raydium_state.protocol_fee_rate(),
            raydium_state.fund_fee_rate(),
        )?;

        log!("Short: Position Open Calculation Results: {:?}", short_calcs);

        let quote_token_amm_fees = short_calcs.total_quote_token_to_swap.safe_sub(collateral_quote_amt)?;
        let excess_base_token_fees = short_calcs.excess_base_token_fees;
        (
            short_calcs.base_token_collateral_amt,
            short_calcs.open_calcs.max_liquidity,
            short_calcs.open_calcs.expected_position_size,
            short_calcs.open_calcs.expected_position_tokens_removed,
            short_calcs.open_calcs.expected_collateral_tokens_removed,
            short_calcs.open_calcs.expected_pool_position_tokens_after,
            short_calcs.open_calcs.expected_pool_collateral_tokens_after,
            None,
            Some(short_calcs),
            excess_base_token_fees,
            quote_token_amm_fees,
        )
    };

    let liquidity_value = calculator::liquidity_value_for_borrow(
        max_liquidity,
        raydium_state.lp_supply(),
        raydium_state.quote_token_amt(&market_account),
    )?;
    let blackwing_fee_quote_amt = calculator::blackwing_fee_quote_token_amt(
        liquidity_value,
        max_liquidity,
        duration_slots,
        market_account.base_fee_apr,
        market_account.lp_tokens_removed_for_positions,
        market_account.lp_tokens_supplied_pool.total_balance(),
        market_account.min_fee_quote_token,
        false,
    )?;

    log!(
        "Quote: open_quote_token_amt: {}, open_base_token_amt: {}, after_open_quote_token_amt: {}, after_open_base_token_amt: {}, \
amm_fee_amt_quote_token: {}, blackwing_fee_amt_quote_token: {}, position_size: {}, loan_base_amt: {}, loan_quote_amt: {}",
        open_quote_token_amt,
        open_base_token_amt,
        match is_short { // pool quote token amount after
            false => pool_collateral_tokens_after,
            true => pool_position_tokens_after,
        },
         match is_short { // pool base token amount after
            false => pool_position_tokens_after,
            true => pool_collateral_tokens_after,
        },
        quote_token_amm_fees,
        blackwing_fee_quote_amt,
        position_size,
        match is_short { // loan base amount
            false => loan_position_tokens,
            true => loan_collateral_tokens,
        },
        match is_short { // loan quote amount
            false => loan_collateral_tokens,
            true => loan_position_tokens,
        },
    );

    let calcs = if is_short {
        let _short_calcs = short_calcs.unwrap();
        // Perform swap.
        wrap_wsol_if_needed(
            account_infos.user_account,
            quote_user_ata_info,
            quote_token_mint,
            _short_calcs.total_quote_token_to_swap,
        )?;
        let base_token_swapped = user_swap_exact_input(
            &account_infos.user_swap_accounts(),
            quote_token_mint,
            _short_calcs.total_quote_token_to_swap,
            _short_calcs.expected_base_token_swap_output,
        )?;
        raydium_state.reload(account_infos.raydium_state_accounts())?;
        let min_base_token_amt = _short_calcs.base_token_collateral_amt.safe_add(_short_calcs.open_calcs.fees.total_fees)?;
        if base_token_swapped < min_base_token_amt {
            log!(
                "InvalidBaseTokenOutput: expected {} actual {}",
                min_base_token_amt, base_token_swapped,
            );
            return Err(LimitlessError::InvalidBaseTokenOutput);
        }

        log!("Short: Swapped {} quote_token for {} base_token", _short_calcs.total_quote_token_to_swap, base_token_swapped);
        log!("Short: Excess base_token {}", _short_calcs.excess_base_token_fees);

        _short_calcs.open_calcs
    } else {
        long_calcs.unwrap()
    };

    if max_raydium_fee_amt_quote != 0 && max_raydium_fee_amt_quote < calcs.fees.total_fees {
        return Err(LimitlessError::MaxRaydiumFeeExceeded);
    }
    if max_blackwing_fee_amt_quote != 0 && blackwing_fee_quote_amt > max_blackwing_fee_amt_quote {
        return Err(LimitlessError::MaxBlackwingFeeExceeded);
    }

    let market_base_token_balance_before = market_state.base_token_balance();
    let market_quote_token_balance_before = market_state.quote_token_balance();

    let expected_open_swap_output = calcs.expected_position_size
        .safe_sub(calcs.expected_position_tokens_removed)?;

    let expected_market_base_token_balance_at_end = if !is_short {
        market_base_token_balance_before
            .safe_add(excess_base_token)?
            .safe_add(calcs.expected_position_size)?
    } else {
        market_base_token_balance_before
            .safe_add(excess_base_token)?
            .safe_add(collateral_amt)?
            .safe_add(calcs.fees.total_fees)?
            .safe_sub(calcs.fees.raydium_open_swap_fee)?
    };

    let expected_market_quote_token_balance_at_end = if !is_short {
        market_quote_token_balance_before
            .safe_add(collateral_amt)?
            .safe_add(blackwing_fee_quote_amt)?
            .safe_add(rollover_reserve_quote_amt)?
            .safe_add(calcs.fees.total_fees)?
            .safe_sub(calcs.fees.raydium_open_swap_fee)?
    } else {
        market_quote_token_balance_before
            .safe_add(blackwing_fee_quote_amt)?
            .safe_add(rollover_reserve_quote_amt)?
            .safe_add(calcs.expected_position_size)?
    };

    let user_base_token_input = if !is_short {
        0
    } else {
        collateral_amt.safe_add(calcs.fees.total_fees)?
    }.safe_add(excess_base_token)?;
    let user_quote_token_input = if !is_short {
        collateral_amt.safe_add(calcs.fees.total_fees)?
    } else {
        0
    }.safe_add(blackwing_fee_quote_amt)?.safe_add(rollover_reserve_quote_amt)?;

    if user_base_token_input > 0 {
        wrap_wsol_if_needed(
            account_infos.user_account,
            base_user_ata_info,
            base_token_mint,
            user_base_token_input,
        )?;
        transfer_spl_from_user_to_market(
            account_infos.user_account,
            base_user_ata_info,
            base_market_ata_info,
            account_infos.token_program,
            user_base_token_input,
        )?;
    }
    if user_quote_token_input > 0 {
        wrap_wsol_if_needed(
            account_infos.user_account,
            quote_user_ata_info,
            quote_token_mint,
            user_quote_token_input,
        )?;
        transfer_spl_from_user_to_market(
            account_infos.user_account,
            quote_user_ata_info,
            quote_market_ata_info,
            account_infos.token_program,
            user_quote_token_input,
        )?;
    }

    // Extract liquidity from the pool.

    let (base_tokens_removed, quote_tokens_removed) = {
        let (token_0_removed, token_1_removed) = remove_liquidity(
            &seeds.market_account,
            &mut market_state,
            &account_infos,
            calcs.max_liquidity,
        )?;
        log!("Actual tokens removed from raydium: tokens_0 {} token_1 {}", token_0_removed, token_1_removed);

        match market_account.quote_token {
            QuoteToken::Token1 => (token_0_removed, token_1_removed),
            QuoteToken::Token0 => (token_1_removed, token_0_removed),
        }
    };

    // Long position.
    let position_size = if !is_short {
        if base_tokens_removed != calcs.expected_position_tokens_removed {
            log!(
                "InvalidBaseTokenRemoved: expected {} actual {}",
                calcs.expected_collateral_tokens_removed, base_tokens_removed,
            );
            return Err(LimitlessError::InvalidBaseTokenRemoved);
        };
        if quote_tokens_removed != calcs.expected_collateral_tokens_removed {
            log!(
                "InvalidQuoteTokenRemoved: expected {} actual {}",
                calcs.expected_collateral_tokens_removed, quote_tokens_removed,
            );
            return Err(LimitlessError::InvalidQuoteTokenRemoved);
        };

        // Perform swap.
        log!(
            "Swapping {} quote_token for at least {} base_token",
            calcs.collateral_tokens_to_use_for_open_swap, expected_open_swap_output,
        );
        let base_token_swapped = market_swap_exact_input(
            &seeds.market_account,
            &account_infos.market_swap_accounts(),
            quote_token_mint,
            calcs.collateral_tokens_to_use_for_open_swap,
            expected_open_swap_output,
        )?;
        log!("Swapped {} quote_token for {} base_token", calcs.collateral_tokens_to_use_for_open_swap, base_token_swapped);
        log!("Expected to swap {} quote_token for {} base_token", calcs.collateral_tokens_to_use_for_open_swap, expected_open_swap_output);

        // Position size is the final amount of base_token minus the amount reserved for the close
        // position fee.
        let position_size = base_tokens_removed.safe_add(base_token_swapped)?;
        if position_size != calcs.expected_position_size {
            log!("InvalidPositionSize: expected {} actual {}", calcs.expected_position_size, position_size);
            return Err(LimitlessError::InvalidPositionSize);
        };

        position_size
    }
    // Short position.
    else {
        if base_tokens_removed != calcs.expected_collateral_tokens_removed {
            log!(
                "InvalidBaseTokenRemoved: expected {} actual {}",
                calcs.expected_collateral_tokens_removed, base_tokens_removed,
            );
            return Err(LimitlessError::InvalidBaseTokenRemoved);
        };
        if quote_tokens_removed != calcs.expected_position_tokens_removed {
            log!(
                "InvalidQuoteTokenRemoved: expected {} actual {}",
                calcs.expected_collateral_tokens_removed, quote_tokens_removed,
            );
            return Err(LimitlessError::InvalidQuoteTokenRemoved);
        };

        // Perform swap.
        log!(
            "Swapping {} base_token for at least {} quote_token",
            calcs.collateral_tokens_to_use_for_open_swap, expected_open_swap_output,
        );
        let quote_token_swapped = market_swap_exact_input(
            &seeds.market_account,
            &account_infos.market_swap_accounts(),
            base_token_mint,
            calcs.collateral_tokens_to_use_for_open_swap,
            expected_open_swap_output,
        )?;
        log!("Swapped {} token_0 for {} token_1", calcs.collateral_tokens_to_use_for_open_swap, quote_token_swapped);
        log!("Expected to swap {} token_0 for {} token_1", calcs.collateral_tokens_to_use_for_open_swap, expected_open_swap_output);

        // Position size is the final amount of quote_token minus the amount reserved for the close
        // position fee.
        let position_size = quote_tokens_removed.safe_add(quote_token_swapped)?;
        if position_size != calcs.expected_position_size {
            log!("InvalidPositionSize: expected {} actual {}", calcs.expected_position_size, position_size);
            return Err(LimitlessError::InvalidPositionSize);
        };

        position_size
    };

    market_state.reload(account_infos.market_state_accounts())?;
    if market_state.base_token_balance() != expected_market_base_token_balance_at_end {
        log!(
            "InvalidToken0BalanceAtEnd: expected {} actual {}",
            expected_market_base_token_balance_at_end, market_state.base_token_balance(),
        );
        return Err(LimitlessError::InvalidBaseTokenBalanceAtEnd);
    }
    if market_state.quote_token_balance() != expected_market_quote_token_balance_at_end {
        log!(
            "InvalidToken1BalanceAtEnd: expected {} actual {}",
            expected_market_quote_token_balance_at_end, market_state.quote_token_balance(),
        );
        return Err(LimitlessError::InvalidQuoteTokenBalanceAtEnd);
    }

    // Get price after.
    raydium_state.reload(account_infos.raydium_state_accounts())?;
    let after_open_base_token_amt = raydium_state.base_token_amt(&market_account);
    let after_open_quote_token_amt = raydium_state.quote_token_amt(&market_account);
    log!(
        "Raydium state after tx: base_token {} quote_token {}",
        after_open_base_token_amt, after_open_quote_token_amt,
    );
    // Check slippage.
    calculator::check_slippage_opening(
        worst_price_num,
        worst_price_den,
        after_open_base_token_amt,
        after_open_quote_token_amt,
        is_short,
    )?;

    market_account.lp_tokens_removed_for_positions = market_account.lp_tokens_removed_for_positions
        .safe_add(calcs.max_liquidity)?;

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

    // Create position.
    let open_block = utils::state::clock()?.slot;
    let close_block = open_block.safe_add(duration_slots)?;
    let pos = PositionAccount {
        id: position_id,
        market_account: account_infos.market_account.key(),
        position_size,
        user_quote_token_collateral_amt: collateral_quote_amt,
        collateral_amt,
        blackwing_fee_reserve_quote_token_amt: blackwing_fee_quote_amt,
        rounding_fee_reserve_collateral_token_amt: calcs.fees.rounding_fee,
        raydium_fee_reserve_collateral_token_amt: calcs.fees.total_fees
            .safe_sub(calcs.fees.raydium_open_swap_fee)?
            .safe_sub(calcs.fees.rounding_fee)?,
        raydium_fees_reserved_amt_quote_token: quote_token_amm_fees,
        base_token_balance: if !is_short {
            position_size
                .safe_add(excess_base_token)?
        } else {
            collateral_amt
                .safe_add(calcs.fees.total_fees)?
                .safe_sub(calcs.fees.raydium_open_swap_fee)?
                .safe_add(excess_base_token)?
        },
        quote_token_balance: if !is_short {
            collateral_amt
                .safe_add(calcs.fees.total_fees)?
                .safe_sub(calcs.fees.raydium_open_swap_fee)?
                .safe_add(blackwing_fee_quote_amt)?
                .safe_add(rollover_reserve_quote_amt)?
        } else {
            position_size
                .safe_add(blackwing_fee_quote_amt)?
                .safe_add(rollover_reserve_quote_amt)?
        },
        lp_tokens_removed: calcs.max_liquidity,
        loan_position_token_amt: calcs.expected_position_tokens_removed,
        loan_collateral_token_amt: calcs.expected_collateral_tokens_removed,
        is_short,
        open_block,
        close_block,
        open_base_token_amt,
        open_quote_token_amt,
        after_open_base_token_amt,
        after_open_quote_token_amt,
        rollover_max_fee_quote_token_amt: max_blackwing_fee_amt_quote,
        rollover_duration_blocks: duration_slots,
        rollover_reserve_quote_token_amt: rollover_reserve_quote_amt,

        space: [0u8; 120],
    };
    let open_position_event = PositionOpenEvent{
        base_token_mint: base_token_mint.key(),
        quote_token_mint: quote_token_mint.key(),
        user_address: account_infos.user_account.key(),
        id: pos.id,
        position_size: pos.position_size,
        user_collateral_amt: pos.user_quote_token_collateral_amt,
        collateral_amt: pos.collateral_amt,
        is_short: pos.is_short,
        lp_tokens_removed: pos.lp_tokens_removed,
        loan_position_token_amt: pos.loan_position_token_amt,
        loan_collateral_token_amt: pos.loan_collateral_token_amt,
        open_block: pos.open_block,
        close_block: pos.close_block,
        open_x: pos.open_base_token_amt,
        open_y: pos.open_quote_token_amt,
        after_open_x: pos.after_open_base_token_amt,
        after_open_y: pos.after_open_quote_token_amt,
        blackwing_fee_reserve_amt: pos.blackwing_fee_reserve_quote_token_amt,
        rollover_max_fee_amt: pos.rollover_max_fee_quote_token_amt,
        rollover_duration_blocks: pos.rollover_duration_blocks,
        rollover_fee_reserve_amt: pos.rollover_reserve_quote_token_amt,
        position_token_balance: if !pos.is_short {
            pos.base_token_balance
        } else {
            pos.quote_token_balance
        },
        collateral_token_balance: if !pos.is_short {
            pos.quote_token_balance
        } else {
            pos.base_token_balance
        },
        raydium_fee_reserve_amt_quote_token: quote_token_amm_fees,
    };
    // Create position account.
    utils::state::create_account_with_signers(
        &pos,
        account_infos.position_account,
        account_infos.user_account,
        account_infos.rent,
        account_infos.system_program,
        &crate::ID,
        &[&seeds.position_account.as_refs()]
    ).map_err(|_| LimitlessError::CreatePositionAccountInvokeFailed)?;

    let event_cpi_ix = emit_cpi_ix(&open_position_event, account_infos.event_authority.key);
    invoke_signed(
        &event_cpi_ix,
        &[account_infos.event_authority.clone()],
        &[&seeds.event_authority.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    Ok(())
}

fn remove_liquidity(
    market_account_signer_seeds: &MarketAccountSignerSeeds,
    market_state: &mut MarketState,
    account_infos: &OpenPositionAccountsInfos,
    liquidity: u64,
) -> Result<(u64, u64), LimitlessError> {
    market_state.reload(account_infos.market_state_accounts())?;
    let before_raydium_lp_balance = market_state.lp_token_balance();
    let before_token_0_market_balance = market_state.token_0_balance();
    let before_token_1_market_balance = market_state.token_1_balance();

    let withdraw_instruction = raydium_cp_swap::cpi::accounts::Withdraw{
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
        memo_program: account_infos.memo_program.clone(),
    };
    raydium_cp_swap::cpi::withdraw(
        CpiContext::new_with_signer(
            account_infos.raydium_program.clone(),
            withdraw_instruction,
            &[&market_account_signer_seeds.as_refs()]
        ),
        liquidity,
        0,
        0,
    ).map_err(|_| LimitlessError::WithdrawLpInvokeFailed )?;

    market_state.reload(account_infos.market_state_accounts())?;
    let lp_balance_check = market_state.lp_token_balance()
        .safe_add(liquidity)?;
    if before_raydium_lp_balance != lp_balance_check {
        return Err(LimitlessError::InvalidLpTokensRemoved);
    };
    let token_0_removed = {
        if before_token_0_market_balance > market_state.token_0_balance() {
            return Err(LimitlessError::InvalidBaseTokenRemoved);
        };
        market_state.token_0_balance()
            .safe_sub(before_token_0_market_balance)?
    };
    let token_1_removed = {
        if before_token_1_market_balance > market_state.token_1_balance() {
            return Err(LimitlessError::InvalidQuoteTokenRemoved);
        };
        market_state.token_1_balance()
            .safe_sub(before_token_1_market_balance)?
    };
    Ok((token_0_removed, token_1_removed))
}

fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
    position_id: &Uuid,
) -> Result<(
    OpenPositionAccountsInfos<'slice, 'info>,
    SignerSeeds,
), LimitlessError> {
    let account_info_iter = &mut accounts.iter();

    // User account.
    let user_account = utils::next_account_info(account_info_iter)?;

    // Market account.
    let market_account_info = utils::next_account_info(account_info_iter)?;
    // Position account.
    let position_account_info = utils::next_account_info(account_info_iter)?;
    // Token information.
    let token_0_mint_info = utils::next_account_info(account_info_iter)?;
    let token_1_mint_info = utils::next_account_info(account_info_iter)?;
    let market_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let market_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let raydium_lp_mint_info = utils::next_account_info(account_info_iter)?;
    let market_raydium_lp_token_ata_info = utils::next_account_info(account_info_iter)?;
    let user_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let user_token_1_ata_info = utils::next_account_info(account_info_iter)?;
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

    validate_signer(user_account, LimitlessError::InvalidSigner)?;

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

    let accounts = OpenPositionAccountsInfos::<'slice, 'info> {
        user_account,
        market_account: market_account_info,
        position_account: position_account_info,
        token_0_mint: token_0_mint_info,
        token_1_mint: token_1_mint_info,
        market_token_0_ata: market_token_0_ata_info,
        market_token_1_ata: market_token_1_ata_info,
        raydium_lp_mint: raydium_lp_mint_info,
        market_raydium_lp_token_ata: market_raydium_lp_token_ata_info,
        user_token_0_ata: user_token_0_ata_info,
        user_token_1_ata: user_token_1_ata_info,
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
        position_account: position_account_signer_seeds,
        event_authority: event_authority_seeds,
    };
    Ok((accounts, signer_seeds))
}
