use anchor_lang::Key;
use solana_program::account_info::AccountInfo;
use uuid::Uuid;
use blackwing_proc_macros::{account_infos_struct, ToAccountMetaList};
use utils::log;
use utils::events::EventAuthoritySignerSeeds;
use utils::instructions::{ToAccountMetaList, AccountKey, WriteableAccountKey, SignerAccountKey};
use utils::numbers::SafeUnsigned;
use utils::raydium::raydium_cp_swap::accounts::PoolState;
use utils::state::Packable;
use crate::errors::LimitlessError;
use crate::state::position::PositionAccount;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, QuoteToken};
use crate::{calculator};
use crate::calculator::{BLACKWING_FEE_DENOM, BLACKWING_PROTOCOL_FEE};
use crate::instructions::utils::{rollover_position, transfer_spl_from_market_to_user, validate_fee_collector_ata};
use crate::state::config::{ConfigAccount, TradingMode};

#[account_infos_struct(RolloverPositionAccountsInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct RolloverPositionAccounts {
    pub user_account: SignerAccountKey,
    pub market_account: WriteableAccountKey,
    pub position_account: WriteableAccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub raydium_lp_mint: AccountKey,
    pub market_token_0_ata: WriteableAccountKey,
    pub market_token_1_ata: WriteableAccountKey,
    pub market_raydium_lp_token_ata: AccountKey,
    pub fee_collector_quote_token_ata: WriteableAccountKey,
    pub raydium_program: AccountKey,
    pub raydium_config: AccountKey,
    pub pool_state: AccountKey,
    pub token_1_vault: AccountKey,
    pub token_program: AccountKey,
    pub limitless_program: AccountKey,
    pub event_authority: AccountKey,
    pub limitless_config: AccountKey,
}

struct SignerSeeds {
    market_account: MarketAccountSignerSeeds,
    event_authority: EventAuthoritySignerSeeds,
}

pub fn process_rollover_position(
    accounts: &[AccountInfo],
    position_id: Uuid,
    max_fee_override_token_1_amt: Option<u64>,
    rollover_duration_override: Option<u64>,
) -> Result<(), LimitlessError> {
    log!("Processing position rollover: id {}", position_id);

    let (
        account_infos,
        raydium_pool_state,
        seeds,
    ) = parse_accounts(accounts, &position_id)?;

    let config_account = ConfigAccount::unpack(account_infos.limitless_config)
        .map_err(|_| LimitlessError::ConfigAccountSerializationFailed)?;
    let mut market_account = MarketAccount::unpack(&account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;
    let mut position_account = PositionAccount::unpack(account_infos.position_account)
        .map_err(|_| LimitlessError::PositionStateSerializationFailed)?;

    if config_account.trading_mode != TradingMode::Enabled {
        return Err(LimitlessError::MarketClosed);
    }
    if market_account.trading_mode != TradingMode::Enabled && market_account.trading_mode != TradingMode::PositionCloseOnly {
        return Err(LimitlessError::MarketClosed);
    }

    validate_fee_collector_ata(
        &account_infos.fee_collector_quote_token_ata,
        &market_account,
        &config_account,
        account_infos.token_0_mint,
        account_infos.token_1_mint,
        account_infos.token_program,
    )?;

    let raydium_token_1_amt = utils::token::amount_from_token_account_info(&account_infos.token_1_vault)?;
    let raydium_lp_token_supply = raydium_pool_state.lp_supply;

    let current_slot = utils::state::clock()?.slot;
    let prorated_fee = calculator::prorated_fee(
        position_account.open_block,
        position_account.rollover_duration_blocks,
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

    let quote_market_ata_info = match market_account.quote_token {
        QuoteToken::Token0 => account_infos.market_token_0_ata,
        QuoteToken::Token1 => account_infos.market_token_1_ata,
    };
    transfer_spl_from_market_to_user(
        &seeds.market_account,
        account_infos.market_account,
        quote_market_ata_info,
        account_infos.fee_collector_quote_token_ata,
        account_infos.token_program,
        blackwing_earned_fee_quote_tokens,
    )?;

    match market_account.quote_token {
        QuoteToken::Token0 => {
            market_account.token_0_fee_balance_pool.incr_balance(lp_earned_fee_quote_tokens)?;
        },
        QuoteToken::Token1 => {
            market_account.token_1_fee_balance_pool.incr_balance(lp_earned_fee_quote_tokens)?;
        },
    }

    let max_fee = max_fee_override_token_1_amt.unwrap_or(position_account.rollover_max_fee_quote_token_amt);
    let rollover_duration = rollover_duration_override.unwrap_or(position_account.rollover_duration_blocks);
    let (_fees_charged, _fees_returned) = rollover_position(
        account_infos.token_0_mint.key,
        account_infos.token_1_mint.key,
        &mut position_account,
        &market_account,
        market_account.lp_tokens_removed_for_positions,
        market_account.lp_tokens_supplied_pool.total_balance(),
        raydium_token_1_amt,
        raydium_lp_token_supply,
        prorated_fee,
        current_slot,
        max_fee,
        rollover_duration,
        account_infos.user_account.key(),
        account_infos.event_authority,
        &seeds.event_authority,
    )?;
    log!("Rolled over position, charged {} quote token, returned {} quote token", _fees_charged, _fees_returned);
    position_account.pack(account_infos.position_account)
        .map_err(|_| LimitlessError::PositionAccountSerializationFailed)?;
    market_account.pack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;

    return Ok(());
}

fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
    position_id: &Uuid,
) -> Result<(
    RolloverPositionAccountsInfos<'slice, 'info>,
    PoolState,
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
    let raydium_lp_mint_info = utils::next_account_info(account_info_iter)?;
    let market_token_0_ata_info = utils::next_account_info(account_info_iter)?;
    let market_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let market_raydium_lp_token_ata_info = utils::next_account_info(account_info_iter)?;
    let fee_collector_quote_token_ata_info = utils::next_account_info(account_info_iter)?;
    // Raydium specific accounts.
    let raydium_program_info = utils::next_account_info(account_info_iter)?;
    let raydium_config_info = utils::next_account_info(account_info_iter)?;
    let pool_state_info = utils::next_account_info(account_info_iter)?;
    let token_1_vault_info = utils::next_account_info(account_info_iter)?;
    // Program and system accounts.
    let token_program_info = utils::next_account_info(account_info_iter)?;
    let limitless_program_info = utils::next_account_info(account_info_iter)?;
    // Event accounts.
    let event_authority_info = utils::next_account_info(account_info_iter)?;
    // Limitless config.
    let limitless_config_info = utils::next_account_info(account_info_iter)?;

    //
    // Validations.
    //

    utils::validate::validate_signer(user_account, LimitlessError::InvalidSigner)?;

    utils::validate::validate_account(token_program_info, &spl_token::ID, LimitlessError::InvalidTokenProgramAccount)?;
    utils::validate::validate_account(limitless_program_info, &crate::ID, LimitlessError::InvalidProgramAccount)?;

    utils::validate::validate_token_mint(token_0_mint_info, token_program_info, LimitlessError::InvalidToken0MintAccount)?;
    utils::validate::validate_token_mint(token_1_mint_info, token_program_info, LimitlessError::InvalidToken1MintAccount)?;

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

    let position_account_signer_seeds = PositionAccount::signer_seeds(
        market_account_info.key,
        user_account.key,
        position_id,
    );
    utils::validate::validate_pda(
        position_account_info,
        &crate::ID,
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
        None,
        None,
    )?;
    utils::validate::validate_account(token_1_vault_info, &pool_state.token1_vault, LimitlessError::UtilsErrorInvalidRaydiumToken1VaultAccount)?;
    utils::validate::validate_account(raydium_lp_mint_info, &pool_state.lp_mint, LimitlessError::UtilsErrorInvalidRaydiumLpMintAccount)?;

    let event_authority_seeds = EventAuthoritySignerSeeds::new(&crate::ID);
    utils::validate::validate_pda_address(
        event_authority_info,
        &crate::ID,
        &event_authority_seeds.as_refs(),
        LimitlessError::InvalidEventAuthority,
    )?;

    let accounts = RolloverPositionAccountsInfos::<'slice, 'info> {
        user_account,
        market_account: market_account_info,
        position_account: position_account_info,
        token_0_mint: token_0_mint_info,
        token_1_mint: token_1_mint_info,
        raydium_lp_mint: raydium_lp_mint_info,
        market_token_0_ata: market_token_0_ata_info,
        market_token_1_ata: market_token_1_ata_info,
        market_raydium_lp_token_ata: market_raydium_lp_token_ata_info,
        fee_collector_quote_token_ata: fee_collector_quote_token_ata_info,
        raydium_program: raydium_program_info,
        raydium_config: raydium_config_info,
        pool_state: pool_state_info,
        token_1_vault: token_1_vault_info,
        token_program: token_program_info,
        limitless_program: limitless_program_info,
        event_authority: event_authority_info,
        limitless_config: limitless_config_info,
    };

    let signer_seeds = SignerSeeds {
        market_account: market_account_signer_seeds,
        event_authority: event_authority_seeds,
    };

    Ok((accounts, pool_state, signer_seeds))
}
