use solana_program::account_info::AccountInfo;
use solana_program::program::invoke_signed;
use blackwing_proc_macros::{ToAccountMetaList, account_infos_struct};
use utils::events::EventAuthoritySignerSeeds;
use utils::instructions::{ToAccountMetaList, AccountKey, SignerAccountKey, WriteableAccountKey};
use utils::state::Packable;
use crate::errors::LimitlessError;
use crate::events::{emit_cpi_ix, UpdateMarketConfigEvent};
use crate::state::config::{ConfigAccount, TradingMode};
use crate::state::market::{MarketAccount, QuoteToken};

#[account_infos_struct(UpdateMarketConfigAccountInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct UpdateMarketConfigAccounts {
    pub user_account: SignerAccountKey,
    pub market_account: WriteableAccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub event_authority: AccountKey,
    pub token_program: AccountKey,
    pub system_program: AccountKey,
    pub program_id: AccountKey,
    pub limitless_config: AccountKey,
}

struct SignerSeeds {
    event_authority: EventAuthoritySignerSeeds,
}

pub fn process_update_market_config(
    accounts: &[AccountInfo],
    trading_mode: Option<TradingMode>,
    base_fee_apr: Option<u64>,
    min_fee_token_1: Option<u64>,
    min_duration_slots: Option<u64>,
    max_duration_slots: Option<u64>,
) -> Result<(), LimitlessError> {
    let (account_infos, seeds) = parse_accounts(accounts)?;

    let mut market = MarketAccount::unpack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;

    if !account_infos.user_account.key.eq(&market.creator)
        && !account_infos.user_account.key.eq(&crate::admin::ID) {

        return Err(LimitlessError::InvalidAdmin);
    }

    if base_fee_apr.is_some() {
        market.base_fee_apr = base_fee_apr.unwrap();
    }
    if min_fee_token_1.is_some() {
        market.min_fee_quote_token = min_fee_token_1.unwrap();
    }
    if max_duration_slots.is_some() {
        market.max_duration_slots = max_duration_slots.unwrap();
        if market.min_duration_slots >= market.max_duration_slots {
            return Err(LimitlessError::InvalidDurationSlots);
        }
    }
    if min_duration_slots.is_some() {
        market.min_duration_slots = min_duration_slots.unwrap();
        if market.min_duration_slots >= market.max_duration_slots {
            return Err(LimitlessError::InvalidDurationSlots);
        }
    }
    if trading_mode.is_some() {
        market.trading_mode = trading_mode.unwrap();
    }

    market.pack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;

    // emit cpi event
    let (base_token_mint, quote_token_mint) = match market.quote_token {
        QuoteToken::Token1 => (account_infos.token_0_mint.key, account_infos.token_1_mint.key),
        QuoteToken::Token0 => (account_infos.token_1_mint.key, account_infos.token_0_mint.key),
    };
    let event = UpdateMarketConfigEvent {
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        trading_mode: market.trading_mode,
        min_duration: market.min_duration_slots,
        max_duration: market.max_duration_slots,
        min_fee: market.min_fee_quote_token,
        base_fee_apr: market.base_fee_apr,
    };
    let event_cpi_ix = emit_cpi_ix(&event, account_infos.event_authority.key);
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
    (UpdateMarketConfigAccountInfos<'slice, 'info>, SignerSeeds),
    LimitlessError
> {
    let account_info_iter = &mut accounts.iter();

    // User account.
    let user_account_info = utils::next_account_info(account_info_iter)?;

    // Market account.
    let market_account_info = utils::next_account_info(account_info_iter)?;
    // Token information.
    let token_0_mint_info = utils::next_account_info(account_info_iter)?;
    let token_1_mint_info = utils::next_account_info(account_info_iter)?;
    // event authority accounts
    let event_authority_info = utils::next_account_info(account_info_iter)?;
    // Program and system accounts.
    let token_program_info = utils::next_account_info(account_info_iter)?;
    let system_program_info = utils::next_account_info(account_info_iter)?;
    let program_id_info = utils::next_account_info(account_info_iter)?;
    // Limitless config.
    let limitless_config_info = utils::next_account_info(account_info_iter)?;

    //
    // Validations.
    //

    utils::validate::validate_signer(user_account_info, LimitlessError::InvalidSigner)?;

    utils::validate::validate_account(program_id_info, &crate::ID, LimitlessError::InvalidProgramAccount)?;
    utils::validate::validate_account(system_program_info, &solana_program::system_program::ID, LimitlessError::InvalidSystemProgramAccount)?;

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

    let event_authority_seeds = EventAuthoritySignerSeeds::new(&crate::ID);
    utils::validate::validate_pda_address(
        event_authority_info,
        &crate::ID,
        &event_authority_seeds.as_refs(),
        LimitlessError::InvalidEventAuthority,
    )?;

    let accounts = UpdateMarketConfigAccountInfos::<'slice, 'info> {
        user_account: user_account_info,
        market_account: market_account_info,
        token_0_mint: token_0_mint_info,
        token_1_mint: token_1_mint_info,
        event_authority: event_authority_info,
        token_program: token_program_info,
        system_program: system_program_info,
        program_id: program_id_info,
        limitless_config: limitless_config_info,
    };

    Ok((accounts, SignerSeeds{
        event_authority: event_authority_seeds,
    }))
}
