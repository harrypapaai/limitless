use std::cmp::min;
use anchor_lang::Key;
use solana_program::account_info::AccountInfo;
use solana_program::program::invoke_signed;
use blackwing_proc_macros::{ToAccountMetaList, account_infos_struct};
use utils::errors::UtilsError;
use utils::events::EventAuthoritySignerSeeds;
use utils::instructions::{ToAccountMetaList, AccountKey, SignerAccountKey, WriteableAccountKey};
use utils::log;
use utils::numbers::SafeUnsigned;
use utils::state::Packable;
use crate::{state};
use crate::errors::LimitlessError;
use crate::events::{DepositOrWithdrawLiquidityEvent, emit_cpi_ix};
use crate::instructions::utils::{emit_market_state_update, redeem_liquidity_position_fees, transfer_spl_from_market_to_user};
use crate::state::config::ConfigAccount;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, MarketState, MarketStateAccounts, QuoteToken};
use crate::state::liquidity_position::LiquidityPositionAccount;

#[account_infos_struct(WithdrawLiquidityAccountInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct WithdrawLiquidityAccounts {
    pub user_account: SignerAccountKey,
    pub liquidity_position_account: WriteableAccountKey,
    pub market_account: WriteableAccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub market_token_0_ata: WriteableAccountKey,
    pub market_token_1_ata: WriteableAccountKey,
    pub user_token_0_ata: WriteableAccountKey,
    pub user_token_1_ata: WriteableAccountKey,
    pub raydium_lp_mint: AccountKey,
    pub market_raydium_lp_token_ata: WriteableAccountKey,
    pub user_raydium_lp_token_ata: WriteableAccountKey,
    pub raydium_program: AccountKey,
    pub raydium_config: AccountKey,
    pub pool_state: AccountKey,
    pub pool_authority: AccountKey,
    pub pool_observation: AccountKey,
    pub token_0_vault: AccountKey,
    pub token_1_vault: AccountKey,
    pub token_program: AccountKey,
    pub system_program: AccountKey,
    pub rent: AccountKey,
    pub limitless_program: AccountKey,
    pub event_authority: AccountKey,
    pub limitless_config: AccountKey,
}

impl<'refs, 'info> WithdrawLiquidityAccountInfos<'refs, 'info> {
    fn market_state_accounts(&'refs self) -> MarketStateAccounts<'refs, 'info> {
        MarketStateAccounts {
            market_account: self.market_account,
            token_0_ata: self.market_token_0_ata,
            token_1_ata: self.market_token_1_ata,
            raydium_lp_ata: self.market_raydium_lp_token_ata,
        }
    }
}

struct SignerSeeds {
    market: MarketAccountSignerSeeds,
    event_authority: EventAuthoritySignerSeeds,
}

pub enum BurnArgs {
    EntirePosition{
        burn_max: bool,
    },
    PartialPosition{
        share_amt: u64,
        burn_max: bool,
    }
}

pub fn process_withdraw_liquidity(
    accounts: &[AccountInfo],
    burn_args: BurnArgs,
    min_received_lp_tokens: u64,
) -> Result<(), LimitlessError> {
    let (account_infos, seeds) = parse_accounts(accounts)?;

    let mut market_account = MarketAccount::unpack(&account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;

    if state::is_data_cleared(&account_infos.liquidity_position_account.data.borrow()) {
        return Err(LimitlessError::LiquidityPositionAccountDoesNotExist);
    }

    let mut position = LiquidityPositionAccount::unpack(&account_infos.liquidity_position_account)
        .map_err(|_| LimitlessError::LiquidityPositionAccountSerializationFailed)?;

    // Redeem any fees.
    redeem_liquidity_position_fees(
        &mut position,
        &mut market_account,
        &seeds.market,
        account_infos.liquidity_position_account,
        &account_infos.market_account,
        account_infos.market_token_0_ata,
        account_infos.market_token_1_ata,
        account_infos.user_token_0_ata,
        account_infos.user_token_1_ata,
        &account_infos.token_program,
    )?;

    let total_lp_tokens_available = market_account.lp_tokens_supplied_pool.total_balance()
        .safe_sub(market_account.lp_tokens_removed_for_positions)?;

    let (shares_to_burn, burn_max) = match burn_args {
        BurnArgs::EntirePosition { burn_max} => (position.lp_token_pool_position.share_token_amt(), burn_max),
        BurnArgs::PartialPosition { share_amt, burn_max} => (share_amt, burn_max),
    };

    if shares_to_burn > position.lp_token_pool_position.share_token_amt() {
        return Err(LimitlessError::InvalidShareAmt);
    }

    let actual_shares_to_burn = {
        let initial_lp_tokens = market_account
            .lp_tokens_supplied_pool
            .balance_redeemed_for_burn_position_shares(shares_to_burn)?;

        if initial_lp_tokens <= total_lp_tokens_available {
            shares_to_burn
        } else if burn_max {
            log!("InsufficientLiquidityAvailable: total_lp_tokens_available: {}, lp_tokens_needed: {}", total_lp_tokens_available, initial_lp_tokens);
            let max_shares = min(
                market_account.lp_tokens_supplied_pool.shares_needed_to_redeem_balance(
                    total_lp_tokens_available,
                )?,
                position.lp_token_pool_position.share_token_amt(),
            );
            log!("Burning max shares: {}", max_shares);
            max_shares
        } else {
            log!("NotEnoughLiquidity, burning max: total_lp_tokens_available: {}, lp_tokens_needed: {}", total_lp_tokens_available, initial_lp_tokens);
            return Err(LimitlessError::NotEnoughLiquidity)
        }
    };

    let lp_tokens_to_transfer = market_account.lp_tokens_supplied_pool.burn_position_shares(
        &mut position.lp_token_pool_position,
        actual_shares_to_burn,
    )?;
    if lp_tokens_to_transfer == 0 {
        return Err(LimitlessError::ShareAmtTooLow);
    }
    if lp_tokens_to_transfer < min_received_lp_tokens {
        log!("SlippageExceeded: min_received_lp_tokens: {}, actual: {}", min_received_lp_tokens, lp_tokens_to_transfer);
        return Err(LimitlessError::SlippageExceeded);
    }

    // Transfer liquidity tokens to the user.
    transfer_spl_from_market_to_user(
        &seeds.market,
        account_infos.market_account,
        account_infos.market_raydium_lp_token_ata,
        account_infos.user_raydium_lp_token_ata,
        account_infos.token_program,
        lp_tokens_to_transfer,
    )?;

    // Update fee balance pools if there are remaining shares the position owns.
    if position.lp_token_pool_position.share_token_amt() > 0 {
        let percent_liquidity_provided_num = position.lp_token_pool_position.share_token_amt();
        let percent_liquidity_provided_den = market_account.lp_tokens_supplied_pool.share_token_supply();

        market_account.lp_token_fee_balance_pool.incr_position_share(
            &mut position.lp_token_fee_pool_position,
            percent_liquidity_provided_num,
            percent_liquidity_provided_den,
        )?;
        market_account.token_0_fee_balance_pool.incr_position_share(
            &mut position.token_0_fee_pool_position,
            percent_liquidity_provided_num,
            percent_liquidity_provided_den,
        )?;
        market_account.token_1_fee_balance_pool.incr_position_share(
            &mut position.token_1_fee_pool_position,
            percent_liquidity_provided_num,
            percent_liquidity_provided_den,
        )?;
    };

    // Update market.
    market_account.pack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;
    let market_state = MarketState::load(account_infos.market_state_accounts())?;
    emit_market_state_update(
        account_infos.token_0_mint.key,
        account_infos.token_1_mint.key,
        &market_account,
        &market_state,
        &account_infos.event_authority,
        &seeds.event_authority,
    )?;

    // Update liquidity position.
    position.pack(account_infos.liquidity_position_account)
        .map_err(|_| LimitlessError::LiquidityPositionAccountSerializationFailed)?;

    let (
        base_token_mint,
        base_token_fee_share_amt,
        base_token_fake_balance,
        quote_token_mint,
        quote_token_fee_share_amt,
        quote_token_fake_balance,
    ) = match market_account.quote_token {
        QuoteToken::Token1 => (
            account_infos.token_0_mint.key,
            position.token_0_fee_pool_position.share_token_amt(),
            position.token_0_fee_pool_position.fake_balance(),
            account_infos.token_1_mint.key,
            position.token_1_fee_pool_position.share_token_amt(),
            position.token_1_fee_pool_position.fake_balance(),
        ),
        QuoteToken::Token0 => (
            account_infos.token_1_mint.key,
            position.token_1_fee_pool_position.share_token_amt(),
            position.token_1_fee_pool_position.fake_balance(),
            account_infos.token_0_mint.key,
            position.token_0_fee_pool_position.share_token_amt(),
            position.token_0_fee_pool_position.fake_balance(),
        ),
    };
    let withdraw_liquidity_event = DepositOrWithdrawLiquidityEvent{
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        user_address: account_infos.user_account.key(),
        is_withdraw: true,
        lp_tokens_change: lp_tokens_to_transfer,
        new_lp_position_share_token_amt: position.lp_token_pool_position.share_token_amt(),

        new_base_token_fee_share_amt: base_token_fee_share_amt,
        new_base_token_fake_balance: base_token_fake_balance,
        new_quote_token_fee_share_amt: quote_token_fee_share_amt,
        new_quote_token_fake_balance: quote_token_fake_balance,
        new_lp_token_fee_share_amt: position.lp_token_fee_pool_position.share_token_amt(),
        new_lp_token_fake_balance: position.lp_token_fee_pool_position.fake_balance(),
    };
    let event_cpi_ix = emit_cpi_ix(&withdraw_liquidity_event, account_infos.event_authority.key);
    invoke_signed(
        &event_cpi_ix,
        &[account_infos.event_authority.clone()],
        &[&seeds.event_authority.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    Ok(())
}

fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
) -> Result<
    (WithdrawLiquidityAccountInfos<'slice, 'info>, SignerSeeds),
    LimitlessError
> {
    let account_info_iter = &mut accounts.iter();

    // User account.
    let user_account_info = utils::next_account_info(account_info_iter)?;
    // Liquidity position account info.
    let liquidity_position_account_info = utils::next_account_info(account_info_iter)?;
    // Market account.
    let market_account_info = utils::next_account_info(account_info_iter)?;
    // Token information.
    let token_0_mint_info = utils::next_account_info(account_info_iter)?;
    let token_1_mint_info = utils::next_account_info(account_info_iter)?;
    let market_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let market_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let user_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let user_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let raydium_lp_mint_info = utils::next_account_info(account_info_iter)?;
    let market_raydium_lp_token_ata_info = utils::next_account_info(account_info_iter)?;
    let user_raydium_lp_token_ata_info = utils::next_account_info(account_info_iter)?;
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

    utils::validate::validate_signer(user_account_info, LimitlessError::InvalidSigner)?;

    utils::validate::validate_account(token_program_info, &spl_token::ID, LimitlessError::InvalidTokenProgramAccount)?;
    utils::validate::validate_account(system_program_info, &solana_program::system_program::ID, LimitlessError::InvalidSystemProgramAccount)?;
    utils::validate::validate_account(rent_info, &solana_program::sysvar::rent::ID, LimitlessError::InvalidRentAccount)?;
    utils::validate::validate_account(limitless_program_info, &crate::ID, LimitlessError::InvalidProgramAccount)?;

    let config_account_signer_seed = ConfigAccount::signer_seeds()?;
    utils::validate::validate_pda(
        limitless_config_info,
        &crate::ID,
        &crate::ID,
        &config_account_signer_seed.as_refs(),
        LimitlessError::InvalidLimitlessConfigAccountPda,
    )?;

    utils::validate::validate_token_mint(
        token_0_mint_info,
        token_program_info,
        LimitlessError::InvalidToken0MintAccount,
    )?;
    utils::validate::validate_token_mint(
        token_1_mint_info,
        token_program_info,
        LimitlessError::InvalidToken1MintAccount,
    )?;

    let market_signer_seeds = MarketAccount::signer_seeds(
        token_0_mint_info.key,
        token_1_mint_info.key,
    )?;
    utils::validate::validate_pda_address(
        market_account_info,
        &crate::ID,
        &market_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketAccountPda,
    )?;
    let market_token_0_account_seeds = MarketAccount::token_account_signer_seeds(
        market_account_info.key,
        token_program_info.key,
        token_0_mint_info.key,
    );
    utils::validate::validate_pda_address(
        market_token_0_ata_info,
        &crate::ID,
        &market_token_0_account_seeds.as_refs(),
        LimitlessError::InvalidMarketToken0Ata,
    )?;
    let market_token_1_account_seeds = MarketAccount::token_account_signer_seeds(
        market_account_info.key,
        token_program_info.key,
        token_1_mint_info.key,
    );
    utils::validate::validate_pda_address(
        market_token_1_ata_info,
        &crate::ID,
        &market_token_1_account_seeds.as_refs(),
        LimitlessError::InvalidMarketToken1Ata,
    )?;
    let market_raydium_lp_token_account_seeds = MarketAccount::token_account_signer_seeds(
        market_account_info.key,
        token_program_info.key,
        raydium_lp_mint_info.key,
    );
    utils::validate::validate_pda_address(
        market_raydium_lp_token_ata_info,
        &crate::ID,
        &market_raydium_lp_token_account_seeds.as_refs(),
        LimitlessError::InvalidMarketRaydiumLpTokenAta,
    )?;

    let liquidity_position_seeds = LiquidityPositionAccount::signer_seeds(
        market_account_info.key,
        user_account_info.key,
    )?;
    utils::validate::validate_pda_address(
        liquidity_position_account_info,
        &crate::ID,
        &liquidity_position_seeds.as_refs(),
        LimitlessError::InvalidLiquidityPositionAccountPda,
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
    utils::validate::validate_account(raydium_lp_mint_info, &pool_state.lp_mint, UtilsError::InvalidRaydiumLpMintAccount)?;

    let event_authority_seeds = EventAuthoritySignerSeeds::new(&crate::ID);
    utils::validate::validate_pda_address(
        event_authority_info,
        &crate::ID,
        &event_authority_seeds.as_refs(),
        LimitlessError::InvalidEventAuthority,
    )?;

    let accounts = WithdrawLiquidityAccountInfos::<'slice, 'info> {
        user_account: user_account_info,
        market_account: market_account_info,
        liquidity_position_account: liquidity_position_account_info,
        token_0_mint: token_0_mint_info,
        token_1_mint: token_1_mint_info,
        market_token_0_ata: market_token_0_ata_info,
        market_token_1_ata: market_token_1_ata_info,
        user_token_0_ata: user_token_0_ata_info,
        user_token_1_ata: user_token_1_ata_info,
        raydium_lp_mint: raydium_lp_mint_info,
        market_raydium_lp_token_ata: market_raydium_lp_token_ata_info,
        user_raydium_lp_token_ata: user_raydium_lp_token_ata_info,
        token_0_vault: token_0_vault_info,
        token_1_vault: token_1_vault_info,
        raydium_program: raydium_program_info,
        raydium_config: raydium_config_info,
        pool_state: pool_state_info,
        pool_authority: pool_authority_info,
        pool_observation: pool_observation_info,
        token_program: token_program_info,
        system_program: system_program_info,
        rent: rent_info,
        limitless_program: limitless_program_info,
        event_authority: event_authority_info,
        limitless_config: limitless_config_info,
    };


    Ok((accounts, SignerSeeds{
        market: market_signer_seeds,
        event_authority: event_authority_seeds,
    }))
}
