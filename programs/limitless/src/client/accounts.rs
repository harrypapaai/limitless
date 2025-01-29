use anchor_spl::token_2022::spl_token_2022;
use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use utils::instructions::DynamicAccountKey;
use crate::errors::LimitlessError;
use crate::state::market::QuoteToken;

//
// Individual pda derivation helpers.
//

pub fn derive_limitless_config_pda() -> Result<Pubkey, LimitlessError> {
    let seeds = crate::state::config::ConfigAccount::signer_seeds()?;
    Pubkey::create_program_address(
        &seeds.as_refs(),
        &crate::ID,
    ).map_err(|_| LimitlessError::DerivePdaError)
}

pub fn derive_market_account_pda(
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
) -> Result<Pubkey, LimitlessError> {
    let seeds = crate::state::market::MarketAccount::signer_seeds(token_0_mint, token_1_mint)?;
    Pubkey::create_program_address(
        &seeds.as_refs(),
        &crate::ID,
    ).map_err(|_| LimitlessError::DerivePdaError)
}

pub fn derive_market_token_account_pda(
    market_account: &Pubkey,
    token_mint: &Pubkey,
) -> Result<Pubkey, LimitlessError> {
    let seeds = crate::state::market::MarketAccount::token_account_signer_seeds(
        market_account,
        &spl_token::ID,
        token_mint,
    );
    Pubkey::create_program_address(
        &seeds.as_refs(),
        &crate::ID,
    ).map_err(|_| LimitlessError::DerivePdaError)
}

pub fn derive_market_intermediate_token_account_pda(
    market_account: &Pubkey,
    token_mint: &Pubkey,
) -> Result<Pubkey, LimitlessError> {
    let seeds = crate::state::market::MarketAccount::intermediate_token_account_signer_seeds(
        market_account,
        &spl_token::ID,
        token_mint,
    );
    Pubkey::create_program_address(
        &seeds.as_refs(),
        &crate::ID,
    ).map_err(|_| LimitlessError::DerivePdaError)
}

pub fn derive_fee_collector_quote_ata(
    fee_collector: &Pubkey,
    quote_token_mint: &Pubkey,
) -> Pubkey {
    get_associated_token_address(fee_collector, quote_token_mint)
}

pub fn derive_limitless_event_authority_pda() -> Result<Pubkey, LimitlessError> {
    let seeds = utils::events::EventAuthoritySignerSeeds::new(&crate::ID);
    Pubkey::create_program_address(
        &seeds.as_refs(),
        &crate::ID,
    ).map_err(|_| LimitlessError::DerivePdaError)
}

pub fn derive_liquidity_position_account_pda(
    market: &Pubkey,
    user: &Pubkey,
) -> Result<Pubkey, LimitlessError> {
    let seeds = crate::state::liquidity_position::LiquidityPositionAccount::signer_seeds(
        market,
        user,
    )?;
    Pubkey::create_program_address(
        &seeds.as_refs(),
        &crate::ID,
    ).map_err(|_| LimitlessError::DerivePdaError)
}

pub fn derive_position_account_pda(
    market: &Pubkey,
    user: &Pubkey,
    id: uuid::Uuid,
) -> Result<Pubkey, LimitlessError> {
    let seeds = crate::state::position::PositionAccount::signer_seeds(market, user, &id);
    Pubkey::create_program_address(&seeds.as_refs(), &crate::ID).map_err(|_| LimitlessError::DerivePdaError)
}

//
// Instruction account helpers.
//

pub fn admin_init_accounts(fee_collector: &Pubkey) -> Result<crate::instructions::admin_init_limitless::AdminInitAccounts, LimitlessError> {
    Ok(crate::instructions::admin_init_limitless::AdminInitAccounts{
        admin_account: crate::admin::ID.into(),
        fee_collector_account: fee_collector.into(),
        config_account: derive_limitless_config_pda()?.into(),
        system_program: solana_program::system_program::ID.into(),
        rent: solana_program::sysvar::rent::ID.into(),
    })
}

pub fn init_market_accounts(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    fee_collector: &Pubkey,
    quote_token: QuoteToken,
) -> Result<crate::instructions::init_market::InitMarketAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let market_token_0_ata = derive_market_token_account_pda(
        &market_account,
        token_0_mint,
    )?;
    let market_token_1_ata = derive_market_token_account_pda(
        &market_account,
        token_1_mint,
    )?;
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint)?;
    let market_raydium_lp_ata = derive_market_token_account_pda(
        &market_account,
        &raydium_lp_mint,
    )?;
    let event_authority = derive_limitless_event_authority_pda()?;
    let fee_collector_quote_ata = derive_fee_collector_quote_ata(
        fee_collector,
        match quote_token {
            QuoteToken::Token0 => token_0_mint,
            QuoteToken::Token1 => token_1_mint,
        },
    );

    Ok(crate::instructions::init_market::InitMarketAccounts{
        user_account: user_account.into(),
        market_account: market_account.into(),
        fee_collector: fee_collector.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        market_token_0_ata: market_token_0_ata.into(),
        market_token_1_ata: market_token_1_ata.into(),
        fee_collector_quote_token_ata: fee_collector_quote_ata.into(),
        raydium_lp_mint: raydium_lp_mint.into(),
        market_raydium_lp_token_ata: market_raydium_lp_ata.into(),
        event_authority: event_authority.into(),
        program: crate::ID.into(),
        raydium_program: utils::raydium::ID.into(),
        raydium_config: utils::raydium::pda::amm_config_pda()?.into(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint)?.into(),
        pool_authority: utils::raydium::pda::authority_pda()?.into(),
        pool_observation: utils::raydium::pda::observation_pda(token_0_mint, token_1_mint)?.into(),
        token_program: spl_token::ID.into(),
        associated_token_program: spl_associated_token_account::ID.into(),
        system_program: solana_program::system_program::ID.into(),
        rent: solana_program::sysvar::rent::ID.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

pub fn update_market_config_accounts(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
) -> Result<crate::instructions::update_market_config::UpdateMarketConfigAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let event_authority = derive_limitless_event_authority_pda()?;
    Ok(crate::instructions::update_market_config::UpdateMarketConfigAccounts{
        user_account: user_account.into(),
        market_account: market_account.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        event_authority: event_authority.into(),
        token_program: spl_token::ID.into(),
        system_program: solana_program::system_program::ID.into(),
        program_id: crate::ID.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

pub fn open_position_accounts(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    position_id: &uuid::Uuid,
) -> Result<crate::instructions::open_position::OpenPositionAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let position_account = derive_position_account_pda(
        &market_account,
        user_account,
        position_id.clone(),
    )?;
    let market_token_0_ata = derive_market_token_account_pda(
        &market_account,
        token_0_mint,
    )?;
    let market_token_1_ata = derive_market_token_account_pda(
        &market_account,
        token_1_mint,
    )?;
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint)?;
    let market_raydium_lp_ata = derive_market_token_account_pda(
        &market_account,
        &raydium_lp_mint,
    )?;
    let token_0_vault = utils::raydium::pda::token_vault_pda(token_0_mint, token_0_mint, token_1_mint)?;
    let token_1_vault = utils::raydium::pda::token_vault_pda(token_1_mint, token_0_mint, token_1_mint)?;
    let user_token_0_ata = get_associated_token_address(
        user_account,
        token_0_mint,
    );
    let user_token_1_ata = get_associated_token_address(
        user_account,
        token_1_mint,
    );
    let event_authority = derive_limitless_event_authority_pda()?;

    Ok(crate::instructions::open_position::OpenPositionAccounts{
        user_account: user_account.into(),
        market_account: market_account.into(),
        position_account: position_account.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        market_token_0_ata: market_token_0_ata.into(),
        market_token_1_ata: market_token_1_ata.into(),
        raydium_lp_mint: raydium_lp_mint.into(),
        market_raydium_lp_token_ata: market_raydium_lp_ata.into(),
        user_token_0_ata: user_token_0_ata.into(),
        user_token_1_ata: user_token_1_ata.into(),
        raydium_program: utils::raydium::ID.into(),
        raydium_config: utils::raydium::pda::amm_config_pda()?.into(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint)?.into(),
        pool_authority: utils::raydium::pda::authority_pda()?.into(),
        pool_observation: utils::raydium::pda::observation_pda(token_0_mint, token_1_mint)?.into(),
        token_0_vault: token_0_vault.into(),
        token_1_vault: token_1_vault.into(),
        token_program: spl_token::ID.into(),
        token_2022_program: spl_token_2022::ID.into(),
        associated_token_account_program: spl_associated_token_account::ID.into(),
        memo_program: spl_memo::ID.into(),
        system_program: solana_program::system_program::ID.into(),
        rent: solana_program::sysvar::rent::ID.into(),
        limitless_program: crate::ID.into(),
        event_authority: event_authority.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

pub fn close_position_accounts(
    user_account: &Pubkey,
    payer_account: &Pubkey,
    is_owner: bool,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    fee_collector_account: &Pubkey,
    quote_token: QuoteToken,
    position_id: &uuid::Uuid,
) -> Result<crate::instructions::close_position::ClosePositionAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let position_account = derive_position_account_pda(
        &market_account,
        user_account,
        position_id.clone(),
    )?;
    let market_token_0_ata = derive_market_token_account_pda(
        &market_account,
        token_0_mint,
    )?;
    let market_token_1_ata = derive_market_token_account_pda(
        &market_account,
        token_1_mint,
    )?;
    let market_intermediate_wsol_ta = derive_market_intermediate_token_account_pda(
        &market_account,
        &spl_token::native_mint::ID,
    )?;
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint)?;
    let market_raydium_lp_ata = derive_market_token_account_pda(
        &market_account,
        &raydium_lp_mint,
    )?;
    let token_0_vault = utils::raydium::pda::token_vault_pda(token_0_mint, token_0_mint, token_1_mint)?;
    let token_1_vault = utils::raydium::pda::token_vault_pda(token_1_mint, token_0_mint, token_1_mint)?;
    let user_token_0_ata = get_associated_token_address(
        user_account,
        token_0_mint,
    );
    let user_token_1_ata = get_associated_token_address(
        user_account,
        token_1_mint,
    );
    let fee_collector_ata = derive_fee_collector_quote_ata(
        fee_collector_account,
        match quote_token {
            QuoteToken::Token0 => token_0_mint,
            QuoteToken::Token1 => token_1_mint,
        },
    );
    let event_authority = derive_limitless_event_authority_pda()?;
    Ok(crate::instructions::close_position::ClosePositionAccounts{
        user_account: DynamicAccountKey::new(user_account.clone(), true, is_owner),
        payer_account: payer_account.into(),
        market_account: market_account.into(),
        position_account: position_account.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        market_token_0_ata: market_token_0_ata.into(),
        market_token_1_ata: market_token_1_ata.into(),
        market_intermediate_wsol_ta: market_intermediate_wsol_ta.into(),
        raydium_lp_mint: raydium_lp_mint.into(),
        market_raydium_lp_token_ata: market_raydium_lp_ata.into(),
        user_token_0_ata: user_token_0_ata.into(),
        user_token_1_ata: user_token_1_ata.into(),
        fee_collector_quote_token_ata: fee_collector_ata.into(),
        raydium_program: utils::raydium::ID.into(),
        raydium_config: utils::raydium::pda::amm_config_pda()?.into(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint)?.into(),
        pool_authority: utils::raydium::pda::authority_pda()?.into(),
        pool_observation: utils::raydium::pda::observation_pda(token_0_mint, token_1_mint)?.into(),
        token_0_vault: token_0_vault.into(),
        token_1_vault: token_1_vault.into(),
        token_program: spl_token::ID.into(),
        token_2022_program: spl_token_2022::ID.into(),
        associated_token_account_program: spl_associated_token_account::ID.into(),
        memo_program: spl_memo::ID.into(),
        system_program: solana_program::system_program::ID.into(),
        rent: solana_program::sysvar::rent::ID.into(),
        limitless_program: crate::ID.into(),
        event_authority: event_authority.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

pub fn deposit_liquidity_accounts(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    user_token_0_ta: &Pubkey,
    user_token_1_ta: &Pubkey,
) -> Result<crate::instructions::deposit_liquidity::DepositLiquidityAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let liquidity_position_account = derive_liquidity_position_account_pda(
        &market_account,
        user_account,
    )?;
    let market_token_0_ata = derive_market_token_account_pda(
        &market_account,
        token_0_mint,
    )?;
    let market_token_1_ata = derive_market_token_account_pda(
        &market_account,
        token_1_mint,
    )?;
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint)?;
    let market_raydium_lp_ata = derive_market_token_account_pda(
        &market_account,
        &raydium_lp_mint,
    )?;
    let token_0_vault = utils::raydium::pda::token_vault_pda(token_0_mint, token_0_mint, token_1_mint)?;
    let token_1_vault = utils::raydium::pda::token_vault_pda(token_1_mint, token_0_mint, token_1_mint)?;
    let user_raydium_lp_token_ata = get_associated_token_address(
        user_account,
        &raydium_lp_mint,
    );
    let event_authority = derive_limitless_event_authority_pda()?;
    Ok(crate::instructions::deposit_liquidity::DepositLiquidityAccounts{
        user_account: user_account.into(),
        market_account: market_account.into(),
        liquidity_position_account: liquidity_position_account.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        market_token_0_ata: market_token_0_ata.into(),
        market_token_1_ata: market_token_1_ata.into(),
        raydium_lp_mint: raydium_lp_mint.into(),
        market_raydium_lp_token_ata: market_raydium_lp_ata.into(),
        user_token_0_ata: user_token_0_ta.into(),
        user_token_1_ata: user_token_1_ta.into(),
        user_raydium_lp_token_ata: user_raydium_lp_token_ata.into(),
        raydium_program: utils::raydium::ID.into(),
        raydium_config: utils::raydium::pda::amm_config_pda()?.into(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint)?.into(),
        pool_authority: utils::raydium::pda::authority_pda()?.into(),
        token_0_vault: token_0_vault.into(),
        token_1_vault: token_1_vault.into(),
        token_program: spl_token::ID.into(),
        token2022_program: spl_token_2022::ID.into(),
        system_program: solana_program::system_program::ID.into(),
        rent: solana_program::sysvar::rent::ID.into(),
        limitless_program: crate::ID.into(),
        event_authority: event_authority.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

pub fn withdraw_liquidity_accounts(
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    user_token_0_ta: &Pubkey,
    user_token_1_ta: &Pubkey,
) -> Result<crate::instructions::withdraw_liquidity::WithdrawLiquidityAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let liquidity_position_account = derive_liquidity_position_account_pda(
        &market_account,
        user_account,
    )?;
    let market_token_0_ata = derive_market_token_account_pda(
        &market_account,
        token_0_mint,
    )?;
    let market_token_1_ata = derive_market_token_account_pda(
        &market_account,
        token_1_mint,
    )?;
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint)?;
    let market_raydium_lp_ata = derive_market_token_account_pda(
        &market_account,
        &raydium_lp_mint,
    )?;
    let token_0_vault = utils::raydium::pda::token_vault_pda(token_0_mint, token_0_mint, token_1_mint)?;
    let token_1_vault = utils::raydium::pda::token_vault_pda(token_1_mint, token_0_mint, token_1_mint)?;
    let user_raydium_lp_token_ata = get_associated_token_address(
        user_account,
        &raydium_lp_mint,
    );
    let event_authority = derive_limitless_event_authority_pda()?;
    Ok(crate::instructions::withdraw_liquidity::WithdrawLiquidityAccounts{
        user_account: user_account.into(),
        market_account: market_account.into(),
        liquidity_position_account: liquidity_position_account.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        market_token_0_ata: market_token_0_ata.into(),
        market_token_1_ata: market_token_1_ata.into(),
        raydium_lp_mint: raydium_lp_mint.into(),
        market_raydium_lp_token_ata: market_raydium_lp_ata.into(),
        user_token_0_ata: user_token_0_ta.into(),
        user_token_1_ata: user_token_1_ta.into(),
        user_raydium_lp_token_ata: user_raydium_lp_token_ata.into(),
        raydium_program: utils::raydium::ID.into(),
        raydium_config: utils::raydium::pda::amm_config_pda()?.into(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint)?.into(),
        pool_authority: utils::raydium::pda::authority_pda()?.into(),
        pool_observation: utils::raydium::pda::observation_pda(token_0_mint, token_1_mint)?.into(),
        token_0_vault: token_0_vault.into(),
        token_1_vault: token_1_vault.into(),
        token_program: spl_token::ID.into(),
        system_program: solana_program::system_program::ID.into(),
        rent: solana_program::sysvar::rent::ID.into(),
        limitless_program: crate::ID.into(),
        event_authority: event_authority.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

pub fn rollover_position_accounts(
    id: uuid::Uuid,
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    fee_collector: &Pubkey,
    quote_token: QuoteToken,
) -> Result<crate::instructions::rollover_position::RolloverPositionAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let position_account = derive_position_account_pda(
        &market_account,
        user_account,
        id,
    )?;
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint)?;
    let market_token_0_ata = derive_market_token_account_pda(
        &market_account,
        &token_0_mint,
    )?;
    let market_token_1_ata = derive_market_token_account_pda(
        &market_account,
        &token_1_mint,
    )?;
    let market_raydium_lp_ata = derive_market_token_account_pda(
        &market_account,
        &raydium_lp_mint,
    )?;
    let fee_collector_quote_token_ata = derive_fee_collector_quote_ata(
        &fee_collector,
        match quote_token {
            QuoteToken::Token0 => token_0_mint,
            QuoteToken::Token1 => token_1_mint,
        }
    );
    let token_1_vault = utils::raydium::pda::token_vault_pda(token_1_mint, token_0_mint, token_1_mint)?;
    let event_authority = derive_limitless_event_authority_pda()?;
    Ok(crate::instructions::rollover_position::RolloverPositionAccounts{
        user_account: user_account.into(),
        market_account: market_account.into(),
        position_account: position_account.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        raydium_lp_mint: raydium_lp_mint.into(),
        market_token_0_ata: market_token_0_ata.into(),
        market_token_1_ata: market_token_1_ata.into(),
        market_raydium_lp_token_ata: market_raydium_lp_ata.into(),
        fee_collector_quote_token_ata: fee_collector_quote_token_ata.into(),
        raydium_program: utils::raydium::ID.into(),
        raydium_config: utils::raydium::pda::amm_config_pda()?.into(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint)?.into(),
        token_1_vault: token_1_vault.into(),
        token_program: spl_token::ID.into(),
        limitless_program: crate::ID.into(),
        event_authority: event_authority.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

pub fn edit_position_rollover_accounts(
    id: uuid::Uuid,
    user_account: &Pubkey,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    quote_token: QuoteToken,
) -> Result<crate::instructions::edit_position_rollover::EditPositionRolloverAccounts, LimitlessError> {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint)?;
    let position_account = derive_position_account_pda(
        &market_account,
        user_account,
        id,
    )?;
    let quote_token_mint = if quote_token == QuoteToken::Token1 {
        token_1_mint
    } else {
        token_0_mint
    };
    let market_quote_token_ata = derive_market_token_account_pda(
        &market_account,
        quote_token_mint,
    )?;
    let user_quote_token_ata = get_associated_token_address(user_account, quote_token_mint);
    let event_authority = derive_limitless_event_authority_pda()?;
    Ok(crate::instructions::edit_position_rollover::EditPositionRolloverAccounts{
        user_account: user_account.into(),
        market_account: market_account.into(),
        position_account: position_account.into(),
        market_quote_token_ata: market_quote_token_ata.into(),
        user_quote_token_ata: user_quote_token_ata.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        token_program: spl_token::ID.into(),
        limitless_program: crate::ID.into(),
        event_authority: event_authority.into(),
        limitless_config: derive_limitless_config_pda()?.into(),
    })
}

#[cfg(test)]
mod test {
    use std::str::FromStr;
    use solana_program::pubkey::Pubkey;

    #[test]
    fn test_derive_limitless_config_pda() {
        let pda = super::derive_limitless_config_pda().unwrap();
        assert_eq!(pda, Pubkey::from_str("Ffc92U3AvFjGtFqFkPZugSb92g76j1cUhTxGEEFwEcgb").unwrap());
    }

    #[test]
    fn test_derive_market_pda() {
        let token_0_mint = Pubkey::from_str("DVCdrDjyzda6Upb5W9Ayrs78Py5r6Xg8PuvXDtjngYLk").unwrap();
        let token_1_mint = Pubkey::from_str("EAkNjcMoZiz6K3qF1zDJjdqP5Y16gdFapmRJxgrjMKVp").unwrap();
        let pda = super::derive_market_account_pda(&token_0_mint, &token_1_mint).unwrap();
        assert_eq!(pda, Pubkey::from_str("7KFzhtSSF7nGsAviZDogVLScYZ3MZxywhxDn8ymY91nW").unwrap());
    }

    #[test]
    fn test_derive_market_token_account_pda() {
        let token_0_mint = Pubkey::from_str("4WCcsrudU3KapqcJr13wHE5EnjPsdVhQukhek5u8Yue1").unwrap();
        let market_pda = Pubkey::from_str("7oaGtF3ZhimXMrLp5WWyJJKMPpnqPLdwNStLEiEWk3mw").unwrap();
        let token_pda = super::derive_market_token_account_pda(&market_pda, &token_0_mint).unwrap();
        assert_eq!(token_pda, Pubkey::from_str("9F7MRpEZBbReVJnLPRWSGkCMZHVxEUZdHrdPg8695RUu").unwrap());
    }

    #[test]
    fn test_derive_fee_collector_quote_ata() {
        let fee_collector = Pubkey::from_str("8dqGf1UtDvXeM6nEtixvkt1zWucCb31ApKZG6veFaa75").unwrap();
        let quote_token_mint = Pubkey::from_str("G3ddhBZAjgFMXBehohpqPiKkTCGnwWR4xjfV9ALzenR6").unwrap();
        let ata = super::derive_fee_collector_quote_ata(&fee_collector, &quote_token_mint);
        assert_eq!(ata, Pubkey::from_str("84ZWMmhPqEWmreDbwXBzRUJH7UPZBvmm2GnTPhYTYHp").unwrap());
    }

    #[test]
    fn test_derive_limitless_event_authority_pda() {
        let pda = super::derive_limitless_event_authority_pda().unwrap();
        assert_eq!(pda, Pubkey::from_str("EiC7rbJHH48hyhWrThUE1EtTb7bxBZ1NG3aojwkbHPRq").unwrap());
    }

    #[test]
    fn test_derive_liquidity_position_account_pda() {
        let market = Pubkey::from_str("EfZXfGRa1Ykmrjpu78GV8396LZXiKBBm4BLhQuq4hQAx").unwrap();
        let user = Pubkey::from_str("BA4kp64sT8sBqPiPEAD7KtWtJM5P9UQDppuRmURXgdnv").unwrap();
        let pda = super::derive_liquidity_position_account_pda(&market, &user).unwrap();
        assert_eq!(pda, Pubkey::from_str("7fQuHT2DXiPH3zGs8AW1c9DQCnAdY6u9zWK6ShD4mvo4").unwrap());
    }

    #[test]
    fn test_derive_position_account_pda() {
        let market = Pubkey::from_str("6NPSM8pCk1N4yF7MtKfTqWKPnJmPjSzNLyXJN9VmPUfm").unwrap();
        let user = Pubkey::from_str("AxMW7mg3Ca7rgFbWQZtAf6oNYHyz8WnA3Jbu6hBH63wU").unwrap();
        let id = uuid::Uuid::from_str("1babd916-4168-457d-9794-3fb3814e286e").unwrap();
        let pda = super::derive_position_account_pda(&market, &user, id).unwrap();
        assert_eq!(pda, Pubkey::from_str("4o6Yh9rxWpVS7ukWBrnbzpoMhSDgC8rcM5DnWx7q7Lox").unwrap());
    }
}
