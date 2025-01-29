#![allow(dead_code)]

use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_program_test::{BanksClientError, processor};
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use spl_associated_token_account::get_associated_token_address;
use limitless::client::accounts::{derive_fee_collector_quote_ata, derive_limitless_config_pda, derive_limitless_event_authority_pda, derive_liquidity_position_account_pda, derive_market_account_pda, derive_market_token_account_pda, derive_position_account_pda};
use limitless::client::instructions::{admin_init_ix, close_position_ix, deposit_liquidity_ix, edit_rollover_position_ix, init_market_ix, mint_and_deposit_liquidity_ix, open_position_ix, rollover_position_ix, update_market_config_ix, withdraw_liquidity_ix};
use limitless::state::market::QuoteToken;
use limitless::state::position::PositionAccount;
use tutils::raydium::instructions::RaydiumInitOpts;
use tutils::{load_limitless_admin_keypair, T};
use tutils::token::TokenMinter;
use utils::instructions::ToAccountMetaList;
use borsh::{BorshSerialize};

pub async fn admin_init_limitless(t: &mut T, fee_collector: &Pubkey) {
    let admin = load_limitless_admin_keypair();
    // Give admin enough lamports to cover rent and tx cost.
    tutils::token::get_lamports(t, &admin.pubkey(), 1_000_000_000).await;
    t.tx(
        vec![admin_init_ix(fee_collector).unwrap()],
        vec![&admin],
    ).await;
}

pub async fn init_market(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    fee_collector: &Pubkey,
    args: limitless::instructions::InitMarketArgs,
) {
    let ix = init_market_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        fee_collector,
        args,
    ).unwrap();
    t.tx(
        vec![ix],
        vec![&user_account],
    ).await;
}

pub async fn update_market_config(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::UpdateMarketConfigArgs,
) -> Result<(), BanksClientError> {
    let ix = update_market_config_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        args,
    ).unwrap();
    t.tx_res(
        vec![ix],
        vec![&user_account],
    ).await
}

pub async fn open_position(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::OpenPositionArgs,
) -> Result<PositionAccount, BanksClientError> {
    let pos_id = args.id;
    let cu_ix = ComputeBudgetInstruction::set_compute_unit_limit(250_000);
    let ix = open_position_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        args,
    ).unwrap();
    t.tx_res(
        vec![cu_ix, ix],
        vec![&user_account],
    ).await?;

    let market_account = derive_market_account_pda(token_0_mint, token_1_mint).unwrap();
    let position_account = derive_position_account_pda(
        &market_account,
        &user_account.pubkey(),
        pos_id,
    ).unwrap();
    Ok(t.get_account::<PositionAccount>(&position_account).await)
}

pub async fn close_position(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::ClosePositionArgs,
) -> Result<(), BanksClientError> {
    let config_account = get_config_account(t).await;
    let market_account = get_market_account(t, token_0_mint, token_1_mint).await;
    let cu_ix = ComputeBudgetInstruction::set_compute_unit_limit(250_000);
    let ix = close_position_ix(
        &user_account.pubkey(),
        &user_account.pubkey(),
        true,
        token_0_mint,
        token_1_mint,
        &config_account.fee_collector,
        market_account.quote_token,
        args,
    ).unwrap();
    t.tx_res(
        vec![cu_ix, ix],
        vec![&user_account],
    ).await?;
    Ok(())
}

pub async fn close_position_as_external(
    t: &mut T,
    user_account: &Pubkey,
    closer_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::ClosePositionArgs,
) -> Result<(), BanksClientError> {
    let config_account = get_config_account(t).await;
    let market_account = get_market_account(t, token_0_mint, token_1_mint).await;
    let ix = close_position_ix(
        user_account,
        &closer_account.pubkey(),
        false,
        token_0_mint,
        token_1_mint,
        &config_account.fee_collector,
        market_account.quote_token,
        args,
    ).unwrap();
    t.tx_res(
        vec![ix],
        vec![closer_account],
    ).await?;
    Ok(())
}

pub async fn deposit_liquidity(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    token_0_amt: u64,
    token_1_amt: u64,
) -> (u64, u64, u64) {
    let pool_state = tutils::raydium::get_pool_state(
        t,
        token_0_mint,
        token_1_mint,
    ).await;
    let (token_0_pool_amt, token_1_pool_amt) = tutils::raydium::get_pool_token_amounts(t, token_0_mint, token_1_mint).await;
    let (lp_tokens_minted, token_0_deposited, token_1_deposited) = limitless::calculator::calculate_lp_token_amt_rounded_down_u128(
        pool_state.lp_supply as u128,
        token_0_pool_amt as u128,
        token_1_pool_amt as u128,
        token_0_amt as u128,
        token_1_amt as u128,
    ).unwrap();
    let lp_tokens_minted_u64 = lp_tokens_minted as u64;
    let token_0_deposited_u64 = token_0_deposited as u64;
    let token_1_deposited_u64 = token_1_deposited as u64;
    mint_and_deposit_lp_tokens(
        t,
        user_account,
        token_0_mint,
        token_1_mint,
        limitless::instructions::MintLpTokensAndDepositArgs{
            lp_token_amt: lp_tokens_minted_u64,
            max_token_0_amt: token_0_deposited_u64,
            max_token_1_amt: token_1_deposited_u64,
        },
    ).await;

    // Return the amount of tokens used.
    (lp_tokens_minted_u64, token_0_deposited_u64, token_1_deposited_u64)
}

pub async fn deposit_lp_tokens(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    lp_tokens: u64,
) {
    let user_token_0_ata = get_associated_token_address(
        &user_account.pubkey(),
        &token_0_mint,
    );
    let user_token_1_ata = get_associated_token_address(
        &user_account.pubkey(),
        &token_1_mint,
    );
    let ix = deposit_liquidity_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        &user_token_0_ata,
        &user_token_1_ata,
        limitless::instructions::DepositLpTokensArgs{
            lp_token_amt: lp_tokens,
        },
    ).unwrap();
    t.tx(
        vec![ix],
        vec![&user_account],
    ).await;
}

pub async fn rollover_position(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::RolloverPositionArgs,
) -> Result<(), BanksClientError> {
    let market = get_market_account(t, token_0_mint, token_1_mint).await;
    let config = get_config_account(t).await;
    let ix = rollover_position_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        &config.fee_collector,
        market.quote_token,
        args,
    ).unwrap();
    t.tx_res(
        vec![ix],
        vec![&user_account],
    ).await
}

pub async fn edit_position_rollover(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::EditPositionRolloverArgs,
) -> Result<(), BanksClientError> {
    let market_account_key = derive_market_account_pda(token_0_mint, token_1_mint).unwrap();
    let market_account = t.get_account
        ::<limitless::state::market::MarketAccount>(&market_account_key)
        .await;
    let ix = edit_rollover_position_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        market_account.quote_token,
        args,
    ).unwrap();
    t.tx_res(vec![ix], vec![&user_account]).await
}

async fn mint_and_deposit_lp_tokens(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::MintLpTokensAndDepositArgs,
) {
    let user_token_0_ata = get_associated_token_address(
        &user_account.pubkey(),
        &token_0_mint,
    );
    let user_token_1_ata = get_associated_token_address(
        &user_account.pubkey(),
        &token_1_mint,
    );
    let ix = mint_and_deposit_liquidity_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        &user_token_0_ata,
        &user_token_1_ata,
        args,
    ).unwrap();
    t.tx(
        vec![ix],
        vec![&user_account],
    ).await;
}

pub async fn withdraw_liquidity(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::WithdrawLpTokensArgs,
) -> Result<u64, BanksClientError>  {
    let raydium_lp_token = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint).unwrap();
    let raydium_lp_token_balance_before = tutils::token::get_token_balance(
        t,
        &raydium_lp_token,
        &user_account.pubkey(),
    ).await;

    let ix = withdraw_liquidity_ix(
        &user_account.pubkey(),
        token_0_mint,
        token_1_mint,
        &get_associated_token_address(&user_account.pubkey(), token_0_mint),
        &get_associated_token_address(&user_account.pubkey(), token_1_mint),
        args,
    ).unwrap();
    t.tx_res(
        vec![ix],
        vec![&user_account],
    ).await?;
    let raydium_lp_token = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint).unwrap();
    let raydium_lp_token_balance_after = tutils::token::get_token_balance(
        t,
        &raydium_lp_token,
        &user_account.pubkey(),
    ).await;
    Ok(raydium_lp_token_balance_after - raydium_lp_token_balance_before)
}

pub async fn withdraw_all_liquidity(
    t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
    args: limitless::instructions::WithdrawAllLpTokensArgs,
) -> Result<u64, BanksClientError> {
    let raydium_lp_token = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint).unwrap();
    let raydium_lp_token_balance_before = tutils::token::get_token_balance(
        t,
        &raydium_lp_token,
        &user_account.pubkey(),
    ).await;
    let accounts = _withdraw_liquidity_accounts(t, user_account, token_0_mint, token_1_mint);
    let mut data = Vec::new();
    let instruction: limitless::instructions::LimitlessInstruction = args.into();
    instruction.serialize(&mut data).unwrap();
    let instr = Instruction {
        program_id: limitless::ID,
        accounts: accounts.to_account_meta_list(),
        data,
    };
    t.tx_res(
        vec![instr],
        vec![&user_account],
    ).await?;
    let raydium_lp_token_balance_after = tutils::token::get_token_balance(
        t,
        &raydium_lp_token,
        &user_account.pubkey(),
    ).await;
    Ok(raydium_lp_token_balance_after - raydium_lp_token_balance_before)
}

fn _withdraw_liquidity_accounts(
    _t: &mut T,
    user_account: &Keypair,
    token_0_mint: &Pubkey,
    token_1_mint: &Pubkey,
) -> limitless::instructions::withdraw_liquidity::WithdrawLiquidityAccounts {
    let market_account = derive_market_account_pda(token_0_mint, token_1_mint).unwrap();
    let liquidity_position_account = derive_liquidity_position_account_pda(
        &market_account,
        &user_account.pubkey(),
    ).unwrap();
    let market_token_0_ata = derive_market_token_account_pda(
        &market_account,
        token_0_mint,
    ).unwrap();
    let market_token_1_ata = derive_market_token_account_pda(
        &market_account,
        token_1_mint,
    ).unwrap();
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(token_0_mint, token_1_mint).unwrap();
    let market_raydium_lp_ata = derive_market_token_account_pda(
        &market_account,
        &raydium_lp_mint,
    ).unwrap();
    let token_0_vault = utils::raydium::pda::token_vault_pda(token_0_mint, token_0_mint, token_1_mint).unwrap();
    let token_1_vault = utils::raydium::pda::token_vault_pda(token_1_mint, token_0_mint, token_1_mint).unwrap();
    let user_token_0_ata = get_associated_token_address(
        &user_account.pubkey(),
        &token_0_mint,
    );
    let user_token_1_ata = get_associated_token_address(
        &user_account.pubkey(),
        &token_1_mint,
    );
    let user_raydium_lp_token_ata = get_associated_token_address(
        &user_account.pubkey(),
        &raydium_lp_mint,
    );
    let event_authority = derive_limitless_event_authority_pda().unwrap();
    limitless::instructions::withdraw_liquidity::WithdrawLiquidityAccounts{
        user_account: user_account.pubkey().into(),
        market_account: market_account.into(),
        liquidity_position_account: liquidity_position_account.into(),
        token_0_mint: token_0_mint.into(),
        token_1_mint: token_1_mint.into(),
        market_token_0_ata: market_token_0_ata.into(),
        market_token_1_ata: market_token_1_ata.into(),
        raydium_lp_mint: raydium_lp_mint.into(),
        market_raydium_lp_token_ata: market_raydium_lp_ata.into(),
        user_token_0_ata: user_token_0_ata.into(),
        user_token_1_ata: user_token_1_ata.into(),
        user_raydium_lp_token_ata: user_raydium_lp_token_ata.into(),
        raydium_program: utils::raydium::ID.into(),
        raydium_config: utils::raydium::pda::amm_config_pda().unwrap().into(),
        pool_state: utils::raydium::pda::pool_state_pda(token_0_mint, token_1_mint).unwrap().into(),
        pool_authority: utils::raydium::pda::authority_pda().unwrap().into(),
        pool_observation: utils::raydium::pda::observation_pda(token_0_mint, token_1_mint).unwrap().into(),
        token_0_vault: token_0_vault.into(),
        token_1_vault: token_1_vault.into(),
        token_program: spl_token::ID.into(),
        system_program: solana_program::system_program::ID.into(),
        rent: solana_program::sysvar::rent::ID.into(),
        limitless_program: limitless::ID.into(),
        event_authority: event_authority.into(),
        limitless_config: derive_limitless_config_pda().unwrap().into(),
    }
}

pub struct SetupParams {
    pub pool_token_0_amt: u64,
    pub pool_token_1_amt: u64,
    pub raydium_trade_fee: u64,
    pub raydium_protocol_fee: u64,
    pub raydium_fund_fee: u64,
    pub quote_token: QuoteToken,
    pub fee_collector_override: Option<Pubkey>,
    pub quote_token_wsol: bool,
}

pub struct SetupResult {
    pub creator: Keypair,
    pub token_0: TokenMinter,
    pub token_1: TokenMinter,
}

pub async fn setup(params: SetupParams) -> (T, SetupResult) {
    let mut t = tutils::new_t()
        .with_program(
            "limitless",
            limitless::ID,
            processor!(limitless::entrypoint::process_instruction),
        )
        .with_raydium(Some(RaydiumInitOpts{
            trade_fee_rate: Some(params.raydium_trade_fee),
            protocol_fee_rate: Some(params.raydium_protocol_fee),
            fund_fee_rate: Some(params.raydium_fund_fee),
        }))
        .build().await;
    let token_0 : TokenMinter;
    let token_1 : TokenMinter;

    if params.quote_token_wsol {
        match params.quote_token {
            QuoteToken::Token0 => {
                token_0 = TokenMinter::new_wsol();
                let mut token_1_keypair = tutils::token::create_mock_token(&mut t, 9).await;
                while token_1_keypair.pubkey() >= token_0.pubkey() {
                    token_1_keypair = tutils::token::create_mock_token(&mut t, 9).await;
                }
                token_1 = TokenMinter::new(token_1_keypair);
            }
            QuoteToken::Token1 => {
                token_1 = TokenMinter::new_wsol();
                let mut token_0_keypair = tutils::token::create_mock_token(&mut t, 9).await;
                while token_0_keypair.pubkey() >= token_1.pubkey() {
                    token_0_keypair = tutils::token::create_mock_token(&mut t, 9).await;
                }
                token_0 = TokenMinter::new(token_0_keypair);
            }
        }
    } else {
        let mut token_0_keypair = tutils::token::create_mock_token(&mut t, 9).await;
        let mut token_1_keypair = tutils::token::create_mock_token(&mut t, 9).await;
        if token_0_keypair.pubkey() >= token_1_keypair.pubkey() {
            let temp = token_0_keypair.insecure_clone();
            token_0_keypair = token_1_keypair;
            token_1_keypair = temp;
        }
        token_0 = TokenMinter::new(token_0_keypair);
        token_1 = TokenMinter::new(token_1_keypair);
    }

    let creator = Keypair::new();

    token_0.mint(&mut t, &creator.pubkey(), params.pool_token_0_amt).await;
    token_1.mint(&mut t, &creator.pubkey(), params.pool_token_1_amt).await;

    let token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
    let token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
    assert_eq!(token_0_balance, params.pool_token_0_amt);
    assert_eq!(token_1_balance, params.pool_token_1_amt);

    // Initialize raydium.
    tutils::raydium::exec_init_market(
        &mut t,
        &creator,
        &token_0.pubkey(),
        &token_1.pubkey(),
        params.pool_token_0_amt,
        params.pool_token_1_amt,
    ).await;

    // Init limitless.
    admin_init_limitless(&mut t, &params.fee_collector_override.unwrap_or(creator.pubkey())).await;

    // Init market.
    init_market(
        &mut t,
        &creator,
        &token_0.pubkey(),
        &token_1.pubkey(),
        &params.fee_collector_override.unwrap_or(creator.pubkey()),
        limitless::instructions::InitMarketArgs{
            trading_mode: limitless::state::config::TradingMode::Enabled,
            quote_token: params.quote_token,
            base_fee_apr: 100000,
            min_fee_quote_token: 1000,
            min_duration_slots: 10,
            max_duration_slots: 10000000000,
        },
    ).await;
    t.move_time_fwd(10).await;

    // Deposit initial LP tokens.
    let lp_token_mint = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
    let creator_lp_token_account = get_associated_token_address(&creator.pubkey(), &lp_token_mint);
    let lp_tokens = tutils::token::get_token_balance_from_token_account(&mut t, &creator_lp_token_account).await;
    deposit_lp_tokens(&mut t, &creator, &token_0.pubkey(), &token_1.pubkey(), lp_tokens).await;

    (t, SetupResult{
        creator,
        token_0,
        token_1,
    })
}

#[cfg(feature = "localnet")]
pub async fn market_to_dest_token_transfer(
    t: &mut T,
    token_0: &Pubkey,
    token_1: &Pubkey,
    transfer_token: &Pubkey,
    dest_token_ata: &Pubkey,
    amt: u64,
) {
    let market_account_key = derive_market_account_pda(token_0, token_1).unwrap();
    let market_transfer_token_ata = derive_market_token_account_pda(&market_account_key, transfer_token).unwrap();
    let accounts = limitless::instructions::test::transfer::MarketToDestTransferForTestAccounts{
        market_account: market_account_key.into(),
        token_0_mint: token_0.into(),
        token_1_mint: token_1.into(),
        dest_transfer_token_ata: dest_token_ata.into(),
        market_transfer_token_ata: market_transfer_token_ata.into(),
        transfer_token_mint: transfer_token.into(),
        token_program: spl_token::ID.into(),
        system_program: solana_program::system_program::ID.into(),
    };
    let mut data = Vec::new();
    let instruction: limitless::instructions::LimitlessInstruction = limitless::instructions::MarketToDestTokenTransferArgs{
        amt,
    }.into();
    instruction.serialize(&mut data).unwrap();
    let instr = Instruction {
        program_id: limitless::ID,
        accounts: accounts.to_account_meta_list(),
        data,
    };
    t.tx(
        vec![instr],
        vec![],
    ).await;
}

pub async fn add_mock_fees_to_market(
    t: &mut T,
    token_0: &TokenMinter,
    token_1: &TokenMinter,
    token_0_fees: u64,
    token_1_fees: u64,
    lp_token_fees: u64,
) {
    // Send tokens to market.
    let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
    let market_token_0_account = derive_market_token_account_pda(&market_account_key, &token_0.pubkey()).unwrap();
    let market_token_1_account = derive_market_token_account_pda(&market_account_key, &token_1.pubkey()).unwrap();
    let raydium_lp_mint = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
    let market_raydium_lp_token_account = derive_market_token_account_pda(&market_account_key, &raydium_lp_mint).unwrap();
    token_0.mint_to_account(t, &market_token_0_account, token_0_fees).await;
    token_1.mint_to_account(t, &market_token_1_account, token_1_fees).await;
    // For LP tokens, we will mint additional LP tokens and transfer them to the market.
    let pool_state = tutils::raydium::get_pool_state(t, &token_0.pubkey(), &token_1.pubkey()).await;
    let (pool_token_0_amt, pool_token_1_amt) = tutils::raydium::get_pool_token_amounts(t, &token_0.pubkey(), &token_1.pubkey()).await;
    let res = utils::raydium::curves::ConstantProductCurve::lp_tokens_to_trading_tokens(
        lp_token_fees as u128,
        pool_state.lp_supply as u128,
        pool_token_0_amt as u128,
        pool_token_1_amt as u128,
        utils::raydium::curves::RoundDirection::Ceiling,
    ).unwrap();
    let payer_clone = t.payer().insecure_clone();
    token_0.mint(t, &payer_clone.pubkey(), res.token_0_amount as u64).await;
    token_1.mint(t, &payer_clone.pubkey(), res.token_1_amount as u64).await;
    tutils::token::create_token_account(t, &payer_clone.pubkey(), &raydium_lp_mint).await;
    tutils::raydium::exec_deposit(t, &payer_clone, &token_0.pubkey(), &token_1.pubkey(), lp_token_fees, res.token_0_amount as u64, res.token_1_amount as u64).await;
    let raydium_lp_creator_account = get_associated_token_address(&t.payer().pubkey(), &raydium_lp_mint);
    tutils::token::transfer_tokens(t, &payer_clone, &raydium_lp_creator_account, &market_raydium_lp_token_account, lp_token_fees).await;

    // Update accounting.
    let mut market_account = get_market_account(t, &token_0.pubkey(), &token_1.pubkey()).await;
    market_account.token_0_fee_balance_pool.incr_balance(token_0_fees).unwrap();
    market_account.token_1_fee_balance_pool.incr_balance(token_1_fees).unwrap();
    market_account.lp_token_fee_balance_pool.incr_balance(lp_token_fees).unwrap();
    t.write_account(&market_account_key, &market_account).await;
}

pub async fn mock_pool_price_approx(
    t: &mut T,
    token_0_mint: &TokenMinter,
    token_1_mint: &TokenMinter,
    quote_token: QuoteToken,
    pool_price_num: u64,
    pool_price_den: u64,
) {

    let (base_token_mint, quote_token_mint) = match quote_token {
        QuoteToken::Token1 => (token_0_mint, token_1_mint),
        QuoteToken::Token0 => (token_1_mint, token_0_mint),
    };

    let token_0_vault_key = utils::raydium::pda::token_vault_pda(&token_0_mint.pubkey(), &token_0_mint.pubkey(), &token_1_mint.pubkey()).unwrap();
    let token_1_vault_key = utils::raydium::pda::token_vault_pda(&token_1_mint.pubkey(), &token_0_mint.pubkey(), &token_1_mint.pubkey()).unwrap();

    let (base_token_vault, quote_token_vault) = match quote_token {
        QuoteToken::Token1 => (token_0_vault_key, token_1_vault_key),
        QuoteToken::Token0 => (token_1_vault_key, token_0_vault_key),
    };

    let (pool_base_token_amt, pool_quote_token_amt) = {
        let pool_state = tutils::raydium::get_pool_state(t, &token_0_mint.pubkey(), &token_1_mint.pubkey()).await;
        let _pool_token_0_amt = tutils::token::get_token_balance_from_token_account(t, &token_0_vault_key).await;
        let _pool_token_1_amt = tutils::token::get_token_balance_from_token_account(t, &token_1_vault_key).await;
        let (pool_token_0_amt, pool_token_1_amt) = pool_state.vault_amount_without_fee(_pool_token_0_amt, _pool_token_1_amt);
        match quote_token {
            QuoteToken::Token1 => (pool_token_0_amt, pool_token_1_amt),
            QuoteToken::Token0 => (pool_token_1_amt, pool_token_0_amt),
        }
    };
    println!("Price before minting mock tokens: {}/{}", pool_quote_token_amt, pool_base_token_amt);

    let pool_base_token_amt_u128 = pool_base_token_amt as u128;
    let pool_quote_token_amt_u128 = pool_quote_token_amt as u128;
    let pool_price_num_u128 = pool_price_num as u128;
    let pool_price_den_u128 = pool_price_den as u128;

    if pool_price_num_u128 * pool_base_token_amt_u128 == pool_quote_token_amt_u128 * pool_price_den_u128 {
        // Target price == actual price. Do nothing.
        println!("Target price already reached");
        return
    } else if pool_price_num_u128 * pool_base_token_amt_u128 > pool_quote_token_amt_u128 * pool_price_den_u128 {
        // Target price > actual price. Need to mint more token_1 to match target.
        let amt_to_mint = pool_base_token_amt_u128 * pool_price_num_u128 / pool_price_den_u128 - pool_quote_token_amt_u128;
        println!("Minting {} quote token", amt_to_mint);
        quote_token_mint.mint_to_account(t, &quote_token_vault, amt_to_mint as u64).await;
    } else {
        // Target price < actual price. Need to mint more token_0 to match target.
        let amt_to_mint = pool_quote_token_amt_u128 * pool_price_den_u128 / pool_price_num_u128 - pool_base_token_amt_u128;
        println!("Minting {} base token", amt_to_mint);
        base_token_mint.mint_to_account(t, &base_token_vault, amt_to_mint as u64).await;
    }

    let (pool_base_token_amt, pool_quote_token_amt) = {
        let pool_state = tutils::raydium::get_pool_state(t, &token_0_mint.pubkey(), &token_1_mint.pubkey()).await;
        let _pool_token_0_amt = tutils::token::get_token_balance_from_token_account(t, &token_0_vault_key).await;
        let _pool_token_1_amt = tutils::token::get_token_balance_from_token_account(t, &token_1_vault_key).await;
        let (pool_token_0_amt, pool_token_1_amt) = pool_state.vault_amount_without_fee(_pool_token_0_amt, _pool_token_1_amt);
        match quote_token {
            QuoteToken::Token1 => (pool_token_0_amt, pool_token_1_amt),
            QuoteToken::Token0 => (pool_token_1_amt, pool_token_0_amt),
        }
    };
    println!("Price after minting mock tokens: {}/{}", pool_quote_token_amt, pool_base_token_amt);
}

pub async fn mock_incr_lp_token_balance(t: &mut T, token_0: &TokenMinter, token_1: &TokenMinter, amt: u64) {
    let raydium_lp_token_mint = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
    let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
    let market_raydium_lp_token_account = derive_market_token_account_pda(&market_account_key, &raydium_lp_token_mint).unwrap();
    // Mint some LP tokens and transfer them to the market.
    let pool_state = tutils::raydium::get_pool_state(t, &token_0.pubkey(), &token_1.pubkey()).await;
    let (pool_token_0_amt, pool_token_1_amt) = tutils::raydium::get_pool_token_amounts(t, &token_0.pubkey(), &token_1.pubkey()).await;
    let res = utils::raydium::curves::ConstantProductCurve::lp_tokens_to_trading_tokens(
        amt as u128,
        pool_state.lp_supply as u128,
        pool_token_0_amt as u128,
        pool_token_1_amt as u128,
        utils::raydium::curves::RoundDirection::Ceiling,
    ).unwrap();
    let payer_clone = t.payer().insecure_clone();
    token_0.mint(t, &payer_clone.pubkey(), res.token_0_amount as u64).await;
    token_1.mint(t, &payer_clone.pubkey(), res.token_1_amount as u64).await;
    tutils::token::create_token_account(t, &payer_clone.pubkey(), &raydium_lp_token_mint).await;
    tutils::raydium::exec_deposit(t, &payer_clone, &token_0.pubkey(), &token_1.pubkey(), amt, res.token_0_amount as u64, res.token_1_amount as u64).await;
    let raydium_lp_creator_account = get_associated_token_address(&t.payer().pubkey(), &raydium_lp_token_mint);
    tutils::token::transfer_tokens(t, &payer_clone, &raydium_lp_creator_account, &market_raydium_lp_token_account, amt).await;

    // Update accounting.
    let mut market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
    market_account.lp_tokens_supplied_pool.incr_balance(amt).unwrap();
    t.write_account(&market_account_key, &market_account).await;
}

pub async fn mock_decr_lp_token_balance(t: &mut T, token_0: &Pubkey, token_1: &Pubkey, amt: u64) {
    let raydium_lp_token_mint = utils::raydium::pda::lp_mint_pda(token_0, token_1).unwrap();
    let creator_raydium_lp_token_ata = get_associated_token_address(&t.payer().pubkey(), &raydium_lp_token_mint);
    // Create the creator lp token ata in case it doesn't exist.
    tutils::token::create_token_account(t, &t.payer().pubkey(), &raydium_lp_token_mint).await;
    // Transfer lp tokens from market to creator.
    market_to_dest_token_transfer(t, token_0, token_1, &raydium_lp_token_mint, &creator_raydium_lp_token_ata, amt).await;
    // Update accounting.
    let market_account_key = derive_market_account_pda(token_0, token_1).unwrap();
    let mut market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
    market_account.lp_tokens_supplied_pool.decr_balance(amt).unwrap();
    t.write_account(&market_account_key, &market_account).await;
}

pub async fn mock_incr_lp_token_used_for_positions(t: &mut T, token_0: &Pubkey, token_1: &Pubkey, amt: u64) {
    let market_account_key = derive_market_account_pda(token_0, token_1).unwrap();
    let mut market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
    market_account.lp_tokens_removed_for_positions += amt;
    assert!(market_account.lp_tokens_supplied_pool.total_balance() >= market_account.lp_tokens_removed_for_positions);
    t.write_account(&market_account_key, &market_account).await;
}

//
// Getters
//

pub async fn get_market_account(
    t: &mut T,
    token_0: &Pubkey,
    token_1: &Pubkey,
) -> limitless::state::market::MarketAccount {
    let market_pda = &derive_market_account_pda(token_0, token_1).unwrap();
    t.get_account::<limitless::state::market::MarketAccount>(&market_pda).await
}

pub async fn get_fee_collector_quote_token_balance(
    t: &mut tutils::T,
    token_0: &Pubkey,
    token_1: &Pubkey,
) -> u64 {
    let config_account = get_config_account(t).await;
    let market_account = get_market_account(t, token_0, token_1).await;
    let fee_collector_ata = derive_fee_collector_quote_ata(
        &config_account.fee_collector,
        match market_account.quote_token {
            QuoteToken::Token0 => token_0,
            QuoteToken::Token1 => token_1,
        },
    );
    tutils::token::get_token_balance_from_token_account(t, &fee_collector_ata).await
}

pub async fn get_config_account(t: &mut T) -> limitless::state::config::ConfigAccount {
    let config_pda = &derive_limitless_config_pda().unwrap();
    t.get_account::<limitless::state::config::ConfigAccount>(&config_pda).await
}

pub async fn get_liquidity_position_account(
    t: &mut T,
    user: &Pubkey,
    token_0: &Pubkey,
    token_1: &Pubkey,
) -> limitless::state::liquidity_position::LiquidityPositionAccount {
    let pos_pda = derive_liquidity_position_account_pda(
        &derive_market_account_pda(token_0, token_1).unwrap(),
        user,
    ).unwrap();
    t.get_account::<limitless::state::liquidity_position::LiquidityPositionAccount>(&pos_pda).await
}
