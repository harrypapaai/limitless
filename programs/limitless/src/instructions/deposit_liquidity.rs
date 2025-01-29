use anchor_lang::Key;
use anchor_spl::token_2022::spl_token_2022;
use solana_program::account_info::AccountInfo;
use solana_program::program::invoke_signed;
use blackwing_proc_macros::{ToAccountMetaList, account_infos_struct};
use utils::errors::UtilsError;
use utils::events::EventAuthoritySignerSeeds;
use utils::instructions::{ToAccountMetaList, AccountKey, SignerAccountKey, WriteableAccountKey};
use utils::log;
use utils::state::Packable;
use crate::{state};
use crate::errors::LimitlessError;
use crate::events::{DepositOrWithdrawLiquidityEvent, emit_cpi_ix};
use crate::instructions::utils::{mint_lp_tokens_for_user, UserMintLpTokenAccounts, redeem_liquidity_position_fees, transfer_spl_from_user_to_market, emit_market_state_update};
use crate::pool::{BalancePoolPosition, PoolPosition};
use crate::state::config::ConfigAccount;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds, MarketState, MarketStateAccounts, QuoteToken};
use crate::state::liquidity_position::{LiquidityPositionAccount, LiquidityPositionAccountSignerSeeds};

#[account_infos_struct(DepositLiquidityAccountInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct DepositLiquidityAccounts {
    pub user_account: SignerAccountKey,
    pub liquidity_position_account: WriteableAccountKey,
    pub market_account: WriteableAccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub market_token_0_ata: WriteableAccountKey,
    pub market_token_1_ata: WriteableAccountKey,
    pub user_token_0_ata: WriteableAccountKey,
    pub user_token_1_ata: WriteableAccountKey,
    pub raydium_lp_mint: WriteableAccountKey,
    pub market_raydium_lp_token_ata: WriteableAccountKey,
    pub user_raydium_lp_token_ata: WriteableAccountKey,
    pub raydium_program: AccountKey,
    pub raydium_config: AccountKey,
    pub pool_state: WriteableAccountKey,
    pub pool_authority: AccountKey,
    pub token_0_vault: WriteableAccountKey,
    pub token_1_vault: WriteableAccountKey,
    pub token_program: AccountKey,
    pub token2022_program: AccountKey,
    pub system_program: AccountKey,
    pub rent: AccountKey,
    pub limitless_program: AccountKey,
    pub event_authority: AccountKey,
    pub limitless_config: AccountKey,
}

impl<'refs, 'info> DepositLiquidityAccountInfos<'refs, 'info> {
    fn market_state_accounts(&'refs self) -> MarketStateAccounts<'refs, 'info> {
        MarketStateAccounts {
            market_account: self.market_account,
            token_0_ata: self.market_token_0_ata,
            token_1_ata: self.market_token_1_ata,
            raydium_lp_ata: self.market_raydium_lp_token_ata,
        }
    }

    fn user_mint_lp_token_accounts(&'refs self) -> UserMintLpTokenAccounts<'refs, 'info> {
        UserMintLpTokenAccounts {
            user_account: self.user_account,
            user_token_0_ata: self.user_token_0_ata,
            user_token_1_ata: self.user_token_1_ata,
            user_raydium_lp_token_ata: self.user_raydium_lp_token_ata,
            token_0_mint: self.token_0_mint,
            token_1_mint: self.token_1_mint,
            raydium_lp_mint: self.raydium_lp_mint,
            token_0_vault: self.token_0_vault,
            token_1_vault: self.token_1_vault,
            token_program: self.token_program,
            token_program2022: self.token2022_program,
            raydium_program: self.raydium_program,
            pool_state: self.pool_state,
            pool_authority: self.pool_authority,
        }
    }
}

struct SignerSeeds {
    market: MarketAccountSignerSeeds,
    liquidity_position: LiquidityPositionAccountSignerSeeds,
    event_authority: EventAuthoritySignerSeeds,
}

pub struct MintArgs {
    pub max_token_0_amt: u64,
    pub max_token_1_amt: u64,
}

pub fn process_deposit_liquidity(
    accounts: &[AccountInfo],
    lp_token_amt: u64,
    mint_args: Option<MintArgs>,
) -> Result<(), LimitlessError> {
    let (account_infos, seeds) = parse_accounts(accounts)?;

    let mut market_account = MarketAccount::unpack(&account_infos.market_account)
        .map_err(|_| LimitlessError::MarketAccountSerializationFailed)?;

    if let Some(m) = mint_args {
        mint_lp_tokens_for_user(
            &account_infos.user_mint_lp_token_accounts(),
            lp_token_amt,
            m.max_token_0_amt,
            m.max_token_1_amt,
        )?;
    };

    let (mut position, needs_creation) =
        if state::is_data_cleared(&account_infos.liquidity_position_account.data.borrow())
            && !account_infos.liquidity_position_account.owner.eq(&crate::ID)
        {
            (LiquidityPositionAccount {
                lp_token_pool_position: PoolPosition::new(),
                lp_token_fee_pool_position: BalancePoolPosition::new(),
                token_0_fee_pool_position: BalancePoolPosition::new(),
                token_1_fee_pool_position: BalancePoolPosition::new(),
                space: [0u8; 128],
            }, true)
        } else {
            let mut position = LiquidityPositionAccount::unpack(&account_infos.liquidity_position_account)
                .map_err(|_| LimitlessError::LiquidityPositionAccountSerializationFailed)?;
            // If LP position already exists, redeem it first.
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
            (position, false)
        };

    let lp_token_pool_share_balance = market_account.lp_tokens_supplied_pool.total_balance();
    let (percent_liquidity_provided_num, percent_liquidity_provided_den) = if lp_token_pool_share_balance == 0 {
        (1, 1)
    } else {
        (lp_token_amt, lp_token_pool_share_balance)
    };
    if percent_liquidity_provided_num == 0 {
        log!("NotEnoughLiquidity: Not enough lp tokens to mint a share");
        return Err(LimitlessError::NotEnoughLiquidity);
    }

    // Transfer liquidity tokens to the market.
    transfer_spl_from_user_to_market(
        account_infos.user_account,
        account_infos.user_raydium_lp_token_ata,
        account_infos.market_raydium_lp_token_ata,
        account_infos.token_program,
        lp_token_amt,
    )?;

    // Record deposit.
    market_account.lp_tokens_supplied_pool
        .incr_position_amt(&mut position.lp_token_pool_position, lp_token_amt)?;
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
    if needs_creation {
        utils::state::create_account_with_signers(
            &position,
            account_infos.liquidity_position_account,
            account_infos.user_account,
            account_infos.rent,
            account_infos.system_program,
            &crate::ID,
            &[&seeds.liquidity_position.as_refs()],
        ).map_err(|_| LimitlessError::CreateLiquidityPositionAccountInvokeFailed)?;
    } else {
        position.pack(account_infos.liquidity_position_account)
            .map_err(|_| LimitlessError::LiquidityPositionAccountSerializationFailed)?;
    }

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
    let deposit_liquidity_event = DepositOrWithdrawLiquidityEvent{
        base_token_mint: base_token_mint.clone(),
        quote_token_mint: quote_token_mint.clone(),
        user_address: account_infos.user_account.key(),
        is_withdraw: false,
        lp_tokens_change: lp_token_amt,
        new_lp_position_share_token_amt: position.lp_token_pool_position.share_token_amt(),

        new_base_token_fee_share_amt: base_token_fee_share_amt,
        new_base_token_fake_balance: base_token_fake_balance,
        new_quote_token_fee_share_amt: quote_token_fee_share_amt,
        new_quote_token_fake_balance: quote_token_fake_balance,
        new_lp_token_fee_share_amt: position.lp_token_fee_pool_position.share_token_amt(),
        new_lp_token_fake_balance: position.lp_token_fee_pool_position.fake_balance(),
    };
    let event_cpi_ix = emit_cpi_ix(&deposit_liquidity_event, account_infos.event_authority.key);
    invoke_signed(
        &event_cpi_ix,
        &[account_infos.event_authority.clone()],
        &[&seeds.event_authority.as_refs()],
    ).map_err(|_| LimitlessError::EmitCpiEventFailed)?;

    Ok(())
}

#[inline(never)]
fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
) -> Result<
    (DepositLiquidityAccountInfos<'slice, 'info>, SignerSeeds),
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
    let token_0_vault_info = utils::next_account_info(account_info_iter)?;
    let token_1_vault_info = utils::next_account_info(account_info_iter)?;
    // Program and system accounts.
    let token_program_info = utils::next_account_info(account_info_iter)?;
    let token2022_program_info = utils::next_account_info(account_info_iter)?;
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
    utils::validate::validate_account(token2022_program_info, &spl_token_2022::ID, LimitlessError::InvalidToken2022ProgramAccount)?;
    utils::validate::validate_account(system_program_info, &solana_program::system_program::ID, LimitlessError::InvalidSystemProgramAccount)?;
    utils::validate::validate_account(rent_info, &solana_program::sysvar::rent::ID, LimitlessError::InvalidRentAccount)?;
    utils::validate::validate_account(limitless_program_info, &crate::ID, LimitlessError::InvalidProgramAccount)?;

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

    let config_account_signer_seed = ConfigAccount::signer_seeds()?;
    utils::validate::validate_pda(
        limitless_config_info,
        &crate::ID,
        &crate::ID,
        &config_account_signer_seed.as_refs(),
        LimitlessError::InvalidLimitlessConfigAccountPda,
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
        None,
    )?;
    utils::validate::validate_account(raydium_lp_mint_info, &pool_state.lp_mint, UtilsError::InvalidRaydiumLpMintAccount)?;

    let event_authority_seeds = EventAuthoritySignerSeeds::new(&crate::ID);
    utils::validate::validate_pda_address(
        event_authority_info,
        &crate::ID,
        &event_authority_seeds.as_refs(),
        LimitlessError::InvalidEventAuthority,
    )?;

    let accounts = DepositLiquidityAccountInfos::<'slice, 'info> {
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
        token_program: token_program_info,
        token2022_program: token2022_program_info,
        system_program: system_program_info,
        rent: rent_info,
        limitless_program: limitless_program_info,
        event_authority: event_authority_info,
        limitless_config: limitless_config_info,
    };


    Ok((accounts, SignerSeeds{
        market: market_signer_seeds,
        liquidity_position: liquidity_position_seeds,
        event_authority: event_authority_seeds,
    }))
}
