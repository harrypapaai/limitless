use std::ops::Deref;
use anchor_lang::Key;
use solana_program::account_info::AccountInfo;
use solana_program::program::invoke_signed;
use blackwing_proc_macros::{ToAccountMetaList, account_infos_struct};
use utils::errors::UtilsError;
use utils::events::EventAuthoritySignerSeeds;
use utils::instructions::{ToAccountMetaList, AccountKey, SignerAccountKey, WriteableAccountKey};
use utils::state::Packable;
use utils::validate::validate_ata_address;
use crate::errors::LimitlessError;
use crate::pool::{BalancePool, Pool};
use crate::events::{emit_cpi_ix, InitMarketEvent, UpdateMarketConfigEvent};
use crate::state::config::{ConfigAccount, TradingMode};
use crate::state::is_data_cleared;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, MarketTokenAccountSignerSeeds, QuoteToken};

#[account_infos_struct(InitMarketAccountInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct InitMarketAccounts {
    pub user_account: SignerAccountKey,
    pub market_account: WriteableAccountKey,
    pub fee_collector: AccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub market_token_0_ata: WriteableAccountKey,
    pub market_token_1_ata: WriteableAccountKey,
    pub fee_collector_quote_token_ata: WriteableAccountKey,
    pub raydium_lp_mint: AccountKey,
    pub market_raydium_lp_token_ata: WriteableAccountKey,
    pub event_authority: AccountKey,
    pub program: AccountKey,
    pub raydium_program: AccountKey,
    pub raydium_config: AccountKey,
    pub pool_state: AccountKey,
    pub pool_authority: AccountKey,
    pub pool_observation: AccountKey,
    pub token_program: AccountKey,
    pub associated_token_program: AccountKey,
    pub system_program: AccountKey,
    pub rent: AccountKey,
    pub limitless_config: AccountKey,
}

struct SignerSeeds {
    market_account: MarketAccountSignerSeeds,
    market_token_0_ata: MarketTokenAccountSignerSeeds,
    market_token_1_ata: MarketTokenAccountSignerSeeds,
    market_raydium_lp_token_ata: MarketTokenAccountSignerSeeds,
    event_authority: EventAuthoritySignerSeeds,
}

pub fn process_init_market(
    accounts: &[AccountInfo],
    trading_mode: TradingMode,
    quote_token: QuoteToken,
    base_fee_apr: u64,
    min_fee_token_1: u64,
    min_duration_slots: u64,
    max_duration_slots: u64,
) -> Result<(), LimitlessError> {
    let (account_infos, seeds) = parse_accounts(accounts)?;

    if min_duration_slots >= max_duration_slots {
        return Err(LimitlessError::InvalidDurationSlots);
    }

    if !is_data_cleared(account_infos.market_account.data.borrow().deref()) {
        return Err(LimitlessError::MarketAlreadyExists);
    }

    // Create market.
    let market = MarketAccount {
        trading_mode,
        quote_token,
        base_fee_apr,
        min_fee_quote_token: min_fee_token_1,
        min_duration_slots,
        max_duration_slots,

        lp_tokens_supplied_pool: Pool::new(),
        lp_tokens_removed_for_positions: 0,
        lp_token_fee_balance_pool: BalancePool::new(),

        token_0_fee_balance_pool: BalancePool::new(),
        token_1_fee_balance_pool: BalancePool::new(),

        creator: account_infos.user_account.key(),

        space: [0u8; 128],
    };

    validate_ata_address(
        &account_infos.fee_collector_quote_token_ata,
        match quote_token {
            QuoteToken::Token0 => account_infos.token_0_mint,
            QuoteToken::Token1 => account_infos.token_1_mint,
        },
        account_infos.token_program,
        account_infos.fee_collector.key,
        LimitlessError::InvalidFeeCollectorAta,
    )?;
    utils::token::create_token_account_idempotent(
        account_infos.fee_collector_quote_token_ata,
        match quote_token {
            QuoteToken::Token0 => account_infos.token_0_mint,
            QuoteToken::Token1 => account_infos.token_1_mint,
        },
        account_infos.fee_collector,
        account_infos.user_account,
        account_infos.token_program,
        account_infos.system_program,
    ).map_err(|_| LimitlessError::CreateTokenAccountInvokeFailed)?;

    utils::state::create_account_with_signers(
        &market,
        account_infos.market_account,
        account_infos.user_account,
        account_infos.rent,
        account_infos.system_program,
        &crate::ID,
        &[&seeds.market_account.as_refs()],
    ).map_err(|_| LimitlessError::CreateMarketAccountInvokeFailed)?;
    market.pack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;

    // Create market token 0 ata.
    utils::token::create_token_account_with_signers(
        account_infos.market_token_0_ata,
        account_infos.token_0_mint,
        account_infos.market_account,
        account_infos.user_account,
        account_infos.token_program,
        account_infos.system_program,
        account_infos.rent,
        &seeds.market_token_0_ata.as_refs(),
        &seeds.market_account.as_refs(),
    ).map_err(|_| LimitlessError::CreateTokenAccountInvokeFailed)?;

    // Create market token 1 ata.
    utils::token::create_token_account_with_signers(
        account_infos.market_token_1_ata,
        account_infos.token_1_mint,
        account_infos.market_account,
        account_infos.user_account,
        account_infos.token_program,
        account_infos.system_program,
        account_infos.rent,
        &seeds.market_token_1_ata.as_refs(),
        &seeds.market_account.as_refs(),
    ).map_err(|_| LimitlessError::CreateTokenAccountInvokeFailed)?;

    // Create market raydium lp token ata.
    utils::token::create_token_account_with_signers(
        account_infos.market_raydium_lp_token_ata,
        account_infos.raydium_lp_mint,
        account_infos.market_account,
        account_infos.user_account,
        account_infos.token_program,
        account_infos.system_program,
        account_infos.rent,
        &seeds.market_raydium_lp_token_ata.as_refs(),
        &seeds.market_account.as_refs(),
    ).map_err(|_| LimitlessError::CreateTokenAccountInvokeFailed)?;

    // emit events
    let (base_token_mint, quote_token_mint) = match quote_token {
        QuoteToken::Token1 => (account_infos.token_0_mint.key, account_infos.token_1_mint.key),
        QuoteToken::Token0 => (account_infos.token_1_mint.key, account_infos.token_0_mint.key),
    };
    let init_market_event = InitMarketEvent {
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        raydium_config: account_infos.raydium_config.key(),
        raydium_pool_state: account_infos.pool_state.key(),
        quote_token,
        min_duration: market.min_duration_slots,
        max_duration: market.max_duration_slots,
        min_fee: market.min_fee_quote_token,
        base_fee_apr: market.base_fee_apr,
        creator: account_infos.user_account.key(),
    };
    invoke_signed(
        &emit_cpi_ix(&init_market_event, account_infos.event_authority.key),
        &[account_infos.event_authority.clone()],
        &[&seeds.event_authority.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    let (base_token_mint, quote_token_mint) = match quote_token {
        QuoteToken::Token1 => (account_infos.token_0_mint.key, account_infos.token_1_mint.key),
        QuoteToken::Token0 => (account_infos.token_1_mint.key, account_infos.token_0_mint.key),
    };
    let update_market_config_event = UpdateMarketConfigEvent {
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        trading_mode,
        min_duration: market.min_duration_slots,
        max_duration: market.max_duration_slots,
        min_fee: market.min_fee_quote_token,
        base_fee_apr: market.base_fee_apr,
    };
    invoke_signed(
        &emit_cpi_ix(&update_market_config_event, account_infos.event_authority.key),
        &[account_infos.event_authority.clone()],
        &[&seeds.event_authority.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    Ok(())
}

fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
) -> Result<
    (InitMarketAccountInfos<'slice, 'info>, SignerSeeds),
    LimitlessError
> {
    let account_info_iter = &mut accounts.iter();

    // User account.
    let user_account_info = utils::next_account_info(account_info_iter)?;

    // Market account.
    let market_account_info = utils::next_account_info(account_info_iter)?;
    // Fee collector.
    let fee_collector_info = utils::next_account_info(account_info_iter)?;
    // Token information.
    let token_0_mint_info = utils::next_account_info(account_info_iter)?;
    let token_1_mint_info = utils::next_account_info(account_info_iter)?;
    let market_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let market_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let fee_collector_quote_token_ata_info = utils::next_account_info(account_info_iter)?;
    let raydium_lp_mint_info = utils::next_account_info(account_info_iter)?;
    let market_raydium_lp_token_ata_info = utils::next_account_info(account_info_iter)?;
    // event authority accounts
    let event_authority_info = utils::next_account_info(account_info_iter)?;
    let program_info = utils::next_account_info(account_info_iter)?;
    // Raydium specific accounts.
    let raydium_program_info = utils::next_account_info(account_info_iter)?;
    let raydium_config_info = utils::next_account_info(account_info_iter)?;
    let pool_state_info = utils::next_account_info(account_info_iter)?;
    let pool_authority_info = utils::next_account_info(account_info_iter)?;
    let pool_observation_info = utils::next_account_info(account_info_iter)?;
    // Program and system accounts.
    let token_program_info = utils::next_account_info(account_info_iter)?;
    let associated_token_program_info = utils::next_account_info(account_info_iter)?;
    let system_program_info = utils::next_account_info(account_info_iter)?;
    let rent_info = utils::next_account_info(account_info_iter)?;
    // Limitless config.
    let limitless_config_info = utils::next_account_info(account_info_iter)?;

    //
    // Validations.
    //

    utils::validate::validate_signer(user_account_info, LimitlessError::InvalidSigner)?;

    utils::validate::validate_account(token_program_info, &spl_token::ID, LimitlessError::InvalidTokenProgramAccount)?;
    utils::validate::validate_account(associated_token_program_info, &spl_associated_token_account::ID, LimitlessError::InvalidAssociatedTokenProgramAccount)?;
    utils::validate::validate_account(system_program_info, &solana_program::system_program::ID, LimitlessError::InvalidSystemProgramAccount)?;
    utils::validate::validate_account(rent_info, &solana_program::sysvar::rent::ID, LimitlessError::InvalidRentAccount)?;

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

    utils::validate::validate_account(program_info, &crate::ID, LimitlessError::InvalidProgramAccount)?;
    let event_authority_seeds = EventAuthoritySignerSeeds::new(&crate::ID);
    utils::validate::validate_pda_address(
        event_authority_info,
        &crate::ID,
        &event_authority_seeds.as_refs(),
        LimitlessError::InvalidEventAuthority,
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

    let accounts = InitMarketAccountInfos::<'slice, 'info> {
        user_account: user_account_info,
        market_account: market_account_info,
        fee_collector: fee_collector_info,
        token_0_mint: token_0_mint_info,
        token_1_mint: token_1_mint_info,
        market_token_0_ata: market_token_0_ata_info,
        market_token_1_ata: market_token_1_ata_info,
        fee_collector_quote_token_ata: fee_collector_quote_token_ata_info,
        raydium_lp_mint: raydium_lp_mint_info,
        market_raydium_lp_token_ata: market_raydium_lp_token_ata_info,
        event_authority: event_authority_info,
        program: program_info,
        raydium_program: raydium_program_info,
        raydium_config: raydium_config_info,
        pool_state: pool_state_info,
        pool_authority: pool_authority_info,
        pool_observation: pool_observation_info,
        token_program: token_program_info,
        associated_token_program: associated_token_program_info,
        system_program: system_program_info,
        rent: rent_info,
        limitless_config: limitless_config_info,
    };

    Ok((accounts, SignerSeeds{
        market_account: market_signer_seeds,
        market_token_0_ata: market_token_0_account_seeds,
        market_token_1_ata: market_token_1_account_seeds,
        market_raydium_lp_token_ata: market_raydium_lp_token_account_seeds,
        event_authority: event_authority_seeds,
    }))
}
