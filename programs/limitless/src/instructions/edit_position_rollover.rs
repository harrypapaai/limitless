use anchor_lang::Key;
use solana_program::account_info::AccountInfo;
use solana_program::program::invoke_signed;
use uuid::Uuid;
use blackwing_proc_macros::{account_infos_struct, ToAccountMetaList};
use utils::{log, numbers::SafeUnsigned};
use utils::events::EventAuthoritySignerSeeds;
use utils::instructions::{ToAccountMetaList, AccountKey, WriteableAccountKey, SignerAccountKey};
use utils::state::Packable;
use crate::errors::LimitlessError;
use crate::events::{EditPositionRolloverEvent, emit_cpi_ix};
use crate::state::position::PositionAccount;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, QuoteToken};
use crate::instructions::utils::{transfer_spl_from_market_to_user, transfer_spl_from_user_to_market};
use crate::state::config::ConfigAccount;

#[account_infos_struct(EditPositionRolloverAccountsInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct EditPositionRolloverAccounts {
    pub user_account: SignerAccountKey,
    pub market_account: AccountKey,
    pub position_account: WriteableAccountKey,
    pub market_quote_token_ata: WriteableAccountKey,
    pub user_quote_token_ata: WriteableAccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub token_program: AccountKey,
    pub limitless_program: AccountKey,
    pub event_authority: AccountKey,
    pub limitless_config: AccountKey,
}

struct SignerSeeds {
    market_account: MarketAccountSignerSeeds,
    event_authority: EventAuthoritySignerSeeds,
}

pub fn process_edit_position_rollover(
    accounts: &[AccountInfo],
    position_id: Uuid,
    max_fee_quote_token_amt: Option<u64>,
    rollover_duration: Option<u64>,
    new_rollover_reserve_quote_token_amt: Option<u64>,
) -> Result<(), LimitlessError> {
    log!("Processing edit position rollover: id {}", position_id);

    let (
        account_infos,
        seeds,
    ) = parse_accounts(accounts, &position_id)?;

    let mut position_account = PositionAccount::unpack(account_infos.position_account)
        .map_err(|_| LimitlessError::PositionStateSerializationFailed)?;

    if max_fee_quote_token_amt.is_none() && rollover_duration.is_none() && new_rollover_reserve_quote_token_amt.is_none() {
        return Ok(());
    }

    if let Some(max_fee) = max_fee_quote_token_amt {
        position_account.rollover_max_fee_quote_token_amt = max_fee;
    }
    if let Some(duration) = rollover_duration {
        position_account.rollover_duration_blocks = duration;
    }
    if let Some(rollover_reserve) = new_rollover_reserve_quote_token_amt {
        let old_rollover_reserve = position_account.rollover_reserve_quote_token_amt;
        position_account.rollover_reserve_quote_token_amt = rollover_reserve;

        if rollover_reserve > old_rollover_reserve {
            let rollover_reserve_diff = rollover_reserve.safe_sub(old_rollover_reserve)?;
            position_account.quote_token_balance = position_account.quote_token_balance.safe_add(rollover_reserve_diff)?;
            transfer_spl_from_user_to_market(
                account_infos.user_account,
                account_infos.user_quote_token_ata,
                account_infos.market_quote_token_ata,
                account_infos.token_program,
                rollover_reserve_diff,
            )?;
        } else if rollover_reserve < old_rollover_reserve {
            let rollover_reserve_diff = old_rollover_reserve.safe_sub(rollover_reserve)?;
            position_account.quote_token_balance = position_account.quote_token_balance.safe_sub(rollover_reserve_diff)?;
            transfer_spl_from_market_to_user(
                &seeds.market_account,
                account_infos.market_account,
                account_infos.market_quote_token_ata,
                account_infos.user_quote_token_ata,
                account_infos.token_program,
                rollover_reserve_diff,
            )?;
        }
    }
    position_account.pack(account_infos.position_account)
        .map_err(|_| LimitlessError::PositionAccountSerializationFailed)?;

    let market_account = MarketAccount::unpack(account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;
    let (base_token_mint, quote_token_mint) = match market_account.quote_token {
        QuoteToken::Token1 => (account_infos.token_0_mint.key, account_infos.token_1_mint.key),
        QuoteToken::Token0 => (account_infos.token_1_mint.key, account_infos.token_0_mint.key),
    };
    let edit_position_rollover = EditPositionRolloverEvent{
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        user_address: account_infos.user_account.key(),
        id: position_id,
        open_block: position_account.open_block,
        rollover_max_fee_amt: position_account.rollover_max_fee_quote_token_amt,
        rollover_duration_blocks: position_account.rollover_duration_blocks,
        rollover_fee_reserve_amt: position_account.rollover_reserve_quote_token_amt,
    };
    let event_cpi_ix = emit_cpi_ix(&edit_position_rollover, account_infos.event_authority.key);
    invoke_signed(
        &event_cpi_ix,
        &[account_infos.event_authority.clone()],
        &[&seeds.event_authority.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    return Ok(());
}

fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
    position_id: &Uuid,
) -> Result<(
    EditPositionRolloverAccountsInfos<'slice, 'info>,
    SignerSeeds,
), LimitlessError> {
    let account_info_iter = &mut accounts.iter();

    // User account.
    let user_account = utils::next_account_info(account_info_iter)?;

    // Market account.
    let market_account_info = utils::next_account_info(account_info_iter)?;
    // Position account.
    let position_account_info = utils::next_account_info(account_info_iter)?;
    // Token 1 ata infos.
    let market_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    let user_token_1_ata_info = utils::next_account_info(account_info_iter)?;
    // Token information.
    let token_0_mint_info = utils::next_account_info(account_info_iter)?;
    let token_1_mint_info = utils::next_account_info(account_info_iter)?;
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

    utils::validate::validate_ata(
        user_token_1_ata_info,
        &token_1_mint_info,
        token_program_info,
        &user_account.key,
        LimitlessError::InvalidUserToken1Ata,
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

    let event_authority_seeds = EventAuthoritySignerSeeds::new(&crate::ID);
    utils::validate::validate_pda_address(
        event_authority_info,
        &crate::ID,
        &event_authority_seeds.as_refs(),
        LimitlessError::InvalidEventAuthority,
    )?;

    let accounts = EditPositionRolloverAccountsInfos::<'slice, 'info> {
        user_account,
        market_account: market_account_info,
        position_account: position_account_info,
        market_quote_token_ata: market_token_1_ata_info,
        user_quote_token_ata: user_token_1_ata_info,
        token_0_mint: token_0_mint_info,
        token_1_mint: token_1_mint_info,
        token_program: token_program_info,
        limitless_program: limitless_program_info,
        event_authority: event_authority_info,
        limitless_config: limitless_config_info,
    };

    let signer_seeds = SignerSeeds {
        market_account: market_account_signer_seeds,
        event_authority: event_authority_seeds,
    };

    Ok((accounts, signer_seeds))
}
