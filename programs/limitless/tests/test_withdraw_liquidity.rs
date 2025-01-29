mod helpers;

use solana_program_test::tokio;
use solana_sdk::signature::{Keypair, Signer};
use limitless::client::accounts::{derive_liquidity_position_account_pda, derive_market_account_pda, derive_market_token_account_pda};
use limitless::errors::LimitlessError;
use limitless::state::market::QuoteToken;

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_withdraw_all() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        // Accrue some mock fees before the user deposits.
        let token_0_fees = 100;
        let token_1_fees = 200;
        let lp_token_fees = 10;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees, token_1_fees, lp_token_fees).await;

        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let (lp_tokens_minted_and_deposited, token_0_used, token_1_used) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        assert_eq!(token_0_used, 50000000);
        assert_eq!(token_1_used, 49_999999999);

        let mut initial_market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        let mut initial_liquidity_position_account = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_all_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawAllLpTokensArgs{
                min_received_lp_tokens: 1581138825, // Shares lost due to rounding.
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 1581138825);

        initial_market_account.lp_tokens_supplied_pool
            .burn_entire_position(&mut initial_liquidity_position_account.lp_token_pool_position)
            .unwrap();
        initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.lp_token_fee_pool_position)
            .unwrap();
        initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_0_fee_pool_position)
            .unwrap();
        initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_1_fee_pool_position)
            .unwrap();

        // Assert liquidity position account changes.
        let liquidity_position_account_after = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(
        liquidity_position_account_after.lp_token_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.lp_token_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_0_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_0_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_1_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_1_fee_pool_position.share_token_amt(),
    );

        // Assert market account changes.
        let market_account_after = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool, initial_market_account.lp_tokens_supplied_pool);
        assert_eq!(market_account_after.lp_token_fee_balance_pool, initial_market_account.lp_token_fee_balance_pool);
        assert_eq!(market_account_after.token_0_fee_balance_pool, initial_market_account.token_0_fee_balance_pool);
        assert_eq!(market_account_after.token_1_fee_balance_pool, initial_market_account.token_1_fee_balance_pool);
        let market_account_key = derive_market_account_pda(
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).unwrap();
        let market_raydium_lp_token_account = derive_market_token_account_pda(
            &market_account_key,
            &raydium_lp_token,
        ).unwrap();
        assert_eq!(
            tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_account).await,
            market_account_after.lp_tokens_supplied_pool.total_balance() + lp_token_fees
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_all_lp_token_balance_increases() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        // Accrue some mock fees before the user deposits.
        let token_0_fees = 100;
        let token_1_fees = 200;
        let lp_token_fees = 10;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees, token_1_fees, lp_token_fees).await;

        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let (lp_tokens_minted_and_deposited, token_0_used, token_1_used) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        assert_eq!(token_0_used, 50000000);
        assert_eq!(token_1_used, 49_999999999);

        let lp_token_increase = 2581138830;
        helpers::mock_incr_lp_token_balance(&mut t, &token_0, &token_1, lp_token_increase).await;

        let mut initial_market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        let mut initial_liquidity_position_account = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_all_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawAllLpTokensArgs{
                min_received_lp_tokens: 1581138825, // Shares lost due to rounding.
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 1704050198);

        initial_market_account.lp_tokens_supplied_pool
            .burn_entire_position(&mut initial_liquidity_position_account.lp_token_pool_position)
            .unwrap();
        initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.lp_token_fee_pool_position)
            .unwrap();
        initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_0_fee_pool_position)
            .unwrap();
        initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_1_fee_pool_position)
            .unwrap();

        // Assert liquidity position account changes.
        let liquidity_position_account_after = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(
        liquidity_position_account_after.lp_token_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.lp_token_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_0_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_0_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_1_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_1_fee_pool_position.share_token_amt(),
    );

        // Assert market account changes.
        let market_account_after = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool, initial_market_account.lp_tokens_supplied_pool);
        assert_eq!(market_account_after.lp_token_fee_balance_pool, initial_market_account.lp_token_fee_balance_pool);
        assert_eq!(market_account_after.token_0_fee_balance_pool, initial_market_account.token_0_fee_balance_pool);
        assert_eq!(market_account_after.token_1_fee_balance_pool, initial_market_account.token_1_fee_balance_pool);
        let market_account_key = derive_market_account_pda(
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).unwrap();
        let market_raydium_lp_token_account = derive_market_token_account_pda(
            &market_account_key,
            &raydium_lp_token,
        ).unwrap();
        assert_eq!(
            tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_account).await,
            market_account_after.lp_tokens_supplied_pool.total_balance() + lp_token_fees
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_all_lp_token_balance_decreases() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        // Accrue some mock fees before the user deposits.
        let token_0_fees = 100;
        let token_1_fees = 200;
        let lp_token_fees = 10;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees, token_1_fees, lp_token_fees).await;

        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let (lp_tokens_minted_and_deposited, token_0_used, token_1_used) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        assert_eq!(token_0_used, 50000000);
        assert_eq!(token_1_used, 49_999999999);

        let lp_token_decrease = 1581138830;
        helpers::mock_decr_lp_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey(), lp_token_decrease).await;

        let mut initial_market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        let mut initial_liquidity_position_account = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_all_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawAllLpTokensArgs{
                min_received_lp_tokens: 1505846500,
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 1505846500);

        initial_market_account.lp_tokens_supplied_pool
            .burn_entire_position(&mut initial_liquidity_position_account.lp_token_pool_position)
            .unwrap();
        initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.lp_token_fee_pool_position)
            .unwrap();
        initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_0_fee_pool_position)
            .unwrap();
        initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_1_fee_pool_position)
            .unwrap();

        // Assert liquidity position account changes.
        let liquidity_position_account_after = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(
        liquidity_position_account_after.lp_token_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.lp_token_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_0_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_0_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_1_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_1_fee_pool_position.share_token_amt(),
    );

        // Assert market account changes.
        let market_account_after = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool, initial_market_account.lp_tokens_supplied_pool);
        assert_eq!(market_account_after.lp_token_fee_balance_pool, initial_market_account.lp_token_fee_balance_pool);
        assert_eq!(market_account_after.token_0_fee_balance_pool, initial_market_account.token_0_fee_balance_pool);
        assert_eq!(market_account_after.token_1_fee_balance_pool, initial_market_account.token_1_fee_balance_pool);
        let market_account_key = derive_market_account_pda(
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).unwrap();
        let market_raydium_lp_token_account = derive_market_token_account_pda(
            &market_account_key,
            &raydium_lp_token,
        ).unwrap();
        assert_eq!(
            tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_account).await,
            market_account_after.lp_tokens_supplied_pool.total_balance() + lp_token_fees
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_all_not_enough_liquidity() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        let lp_supply_before_deposit = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key)
            .await
            .lp_tokens_supplied_pool
            .total_balance();

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;

        helpers::mock_incr_lp_token_used_for_positions(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
            lp_supply_before_deposit + lp_tokens_minted_and_deposited/2,
        ).await;

        let res = helpers::withdraw_all_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawAllLpTokensArgs{
                min_received_lp_tokens: 0,
                burn_max: false,
            },
        ).await;
        tutils::assert_err(res, LimitlessError::NotEnoughLiquidity);
    }

    #[tokio::test]
    async fn test_withdraw_all_burn_max() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account_key, &raydium_lp_token).unwrap();

        let new_user = Keypair::new();
        let liquidity_position_account = derive_liquidity_position_account_pda(&market_account_key, &new_user.pubkey()).unwrap();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let lp_supply_before_deposit = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key)
            .await
            .lp_tokens_supplied_pool
            .total_balance();
        assert_eq!(lp_supply_before_deposit, 31622776501);

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);

        helpers::mock_incr_lp_token_used_for_positions(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
            lp_supply_before_deposit + lp_tokens_minted_and_deposited/2,
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_all_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawAllLpTokensArgs{
                min_received_lp_tokens: 0,
                burn_max: true,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 793731690);
        let share_amt_after = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt_after, 249);

        let expected_lp_tokens = lp_supply_before_deposit + lp_tokens_minted_and_deposited - lp_tokens_redeemed;
        let market_account_after = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool.total_balance(), expected_lp_tokens);
        assert_eq!(
        tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await,
        expected_lp_tokens,
    );

        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_all_burn_max_not_needed() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account_key, &raydium_lp_token).unwrap();

        let new_user = Keypair::new();
        let liquidity_position_account = derive_liquidity_position_account_pda(&market_account_key, &new_user.pubkey()).unwrap();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let lp_supply_before_deposit = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key)
            .await
            .lp_tokens_supplied_pool
            .total_balance();
        assert_eq!(lp_supply_before_deposit, 31622776501);

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);

        helpers::mock_incr_lp_token_used_for_positions(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
            lp_supply_before_deposit,
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_all_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawAllLpTokensArgs{
                min_received_lp_tokens: 0,
                burn_max: true,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 1581138825);
        let share_amt_after = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt_after, 0);

        let expected_lp_tokens = lp_supply_before_deposit + lp_tokens_minted_and_deposited - lp_tokens_redeemed;
        let market_account_after = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool.total_balance(), expected_lp_tokens);
        assert_eq!(
        tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await,
        expected_lp_tokens,
    );

        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_all_with_fees() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        // Accrue some mock fees before the user deposits.
        let token_0_fees_before_deposit = 100;
        let token_1_fees_before_deposit = 200;
        let lp_token_fees_before_deposit = 10;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees_before_deposit, token_1_fees_before_deposit, lp_token_fees_before_deposit).await;

        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let (lp_tokens_minted_and_deposited, token_0_used, token_1_used) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        assert_eq!(token_0_used, 50000000);
        assert_eq!(token_1_used, 49_999999999);

        // Accrue some mock fees after the user deposits.
        let token_0_fees_after_deposit = 200;
        let token_1_fees_after_deposit = 400;
        let lp_token_fees_after_deposit = 70;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees_after_deposit, token_1_fees_after_deposit, lp_token_fees_after_deposit).await;

        let mut initial_market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        let mut initial_liquidity_position_account = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_all_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawAllLpTokensArgs{
                min_received_lp_tokens: 1581138825, // Shares lost due to rounding.
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 1581138825);

        // When withdrawing, accrued fees will be redeemed.
        let lp_token_fees_redeemed = initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.lp_token_fee_pool_position)
            .unwrap();
        let token_0_fees_redeemed = initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_0_fee_pool_position)
            .unwrap();
        let token_1_fees_redeemed = initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_1_fee_pool_position)
            .unwrap();
        assert_eq!(lp_token_fees_redeemed, 3);
        assert_eq!(token_0_fees_redeemed, 9);
        assert_eq!(token_1_fees_redeemed, 19);
        initial_market_account.lp_tokens_supplied_pool
            .incr_position_amt(&mut initial_liquidity_position_account.lp_token_pool_position, lp_token_fees_redeemed)
            .unwrap();

        initial_market_account.lp_tokens_supplied_pool
            .burn_entire_position(&mut initial_liquidity_position_account.lp_token_pool_position)
            .unwrap();
        initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.lp_token_fee_pool_position)
            .unwrap();
        initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_0_fee_pool_position)
            .unwrap();
        initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_1_fee_pool_position)
            .unwrap();

        // Assert liquidity position account changes.
        let liquidity_position_account_after = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(
        liquidity_position_account_after.lp_token_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.lp_token_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.lp_token_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_0_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_0_fee_pool_position.share_token_amt(),
    );
        assert_eq!(
        liquidity_position_account_after.token_1_fee_pool_position.share_token_amt(),
        initial_liquidity_position_account.token_1_fee_pool_position.share_token_amt(),
    );

        // Assert market account changes.
        let market_account_after = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool, initial_market_account.lp_tokens_supplied_pool);
        assert_eq!(market_account_after.lp_token_fee_balance_pool, initial_market_account.lp_token_fee_balance_pool);
        assert_eq!(market_account_after.token_0_fee_balance_pool, initial_market_account.token_0_fee_balance_pool);
        assert_eq!(market_account_after.token_1_fee_balance_pool, initial_market_account.token_1_fee_balance_pool);
        let market_account_key = derive_market_account_pda(
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).unwrap();
        let market_raydium_lp_token_account = derive_market_token_account_pda(
            &market_account_key,
            &raydium_lp_token,
        ).unwrap();
        assert_eq!(
            tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_account).await,
            // Redeemed LP fees get added to the pool.
            market_account_after.lp_tokens_supplied_pool.total_balance() + lp_token_fees_after_deposit - lp_token_fees_redeemed + lp_token_fees_before_deposit
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used + token_0_fees_redeemed);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used + token_1_fees_redeemed);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    //
    // Partial withdrawals.
    //

    #[tokio::test]
    async fn test_withdraw_partial() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        let liquidity_position_account = derive_liquidity_position_account_pda(
            &derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap(),
            &new_user.pubkey(),
        ).unwrap();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        // Accrue some mock fees before the user deposits.
        let token_0_fees = 100;
        let token_1_fees = 200;
        let lp_token_fees = 10;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees, token_1_fees, lp_token_fees).await;

        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let (lp_tokens_minted_and_deposited, token_0_used, token_1_used) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        assert_eq!(token_0_used, 50000000);
        assert_eq!(token_1_used, 49_999999999);
        let share_amt = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt, 500);

        let mut initial_market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        let mut initial_liquidity_position_account = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 250,
                min_received_lp_tokens: 790569412,
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 790569412);

        initial_market_account.lp_tokens_supplied_pool
            .burn_position_shares(&mut initial_liquidity_position_account.lp_token_pool_position, 250)
            .unwrap();
        initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.lp_token_fee_pool_position)
            .unwrap();
        initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_0_fee_pool_position)
            .unwrap();
        initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_1_fee_pool_position)
            .unwrap();
        let (new_pool_share_percent_num, new_pool_share_percent_den) = (
            250,
            initial_market_account.lp_tokens_supplied_pool.share_token_supply(),
        );
        initial_market_account.lp_token_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position_account.lp_token_fee_pool_position,
                new_pool_share_percent_num,
                new_pool_share_percent_den,
            ).unwrap();
        initial_market_account.token_0_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position_account.token_0_fee_pool_position,
                new_pool_share_percent_num,
                new_pool_share_percent_den,
            ).unwrap();
        initial_market_account.token_1_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position_account.token_1_fee_pool_position,
                new_pool_share_percent_num,
                new_pool_share_percent_den,
            ).unwrap();

        // Assert liquidity position account changes.
        let liquidity_position_account_after = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(liquidity_position_account_after,initial_liquidity_position_account);

        // Assert market account changes.
        let market_account_after = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool, initial_market_account.lp_tokens_supplied_pool);
        assert_eq!(market_account_after.lp_token_fee_balance_pool, initial_market_account.lp_token_fee_balance_pool);
        assert_eq!(market_account_after.token_0_fee_balance_pool, initial_market_account.token_0_fee_balance_pool);
        assert_eq!(market_account_after.token_1_fee_balance_pool, initial_market_account.token_1_fee_balance_pool);
        let market_account_key = derive_market_account_pda(
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).unwrap();
        let market_raydium_lp_token_account = derive_market_token_account_pda(
            &market_account_key,
            &raydium_lp_token,
        ).unwrap();
        assert_eq!(
            tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_account).await,
            market_account_after.lp_tokens_supplied_pool.total_balance() + lp_token_fees
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_partial_with_fees() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        let liquidity_position_account = derive_liquidity_position_account_pda(
            &derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap(),
            &new_user.pubkey(),
        ).unwrap();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        // Accrue some mock fees before the user deposits.
        let token_0_fees_before_deposit = 100;
        let token_1_fees_before_deposit = 200;
        let lp_token_fees_before_deposit = 10;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees_before_deposit, token_1_fees_before_deposit, lp_token_fees_before_deposit).await;

        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let (lp_tokens_minted_and_deposited, token_0_used, token_1_used) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        assert_eq!(token_0_used, 50000000);
        assert_eq!(token_1_used, 49_999999999);
        let share_amt = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt, 500);

        // Accrue some mock fees after the user deposits.
        let token_0_fees_after_deposit = 200;
        let token_1_fees_after_deposit = 400;
        let lp_token_fees_after_deposit = 70;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees_after_deposit, token_1_fees_after_deposit, lp_token_fees_after_deposit).await;

        let mut initial_market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        let mut initial_liquidity_position_account = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 250,
                min_received_lp_tokens: 790569412,
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 790569412);
        initial_market_account.lp_tokens_supplied_pool
            .burn_position_shares(&mut initial_liquidity_position_account.lp_token_pool_position, 250)
            .unwrap();
        let redeemed_lp_token_fees = initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.lp_token_fee_pool_position)
            .unwrap();
        let redeemed_token_0_fees = initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_0_fee_pool_position)
            .unwrap();
        let redeemed_token_1_fees = initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position_account.token_1_fee_pool_position)
            .unwrap();
        let (new_pool_share_percent_num, new_pool_share_percent_den) = (
            250,
            initial_market_account.lp_tokens_supplied_pool.share_token_supply(),
        );
        initial_market_account.lp_token_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position_account.lp_token_fee_pool_position,
                new_pool_share_percent_num,
                new_pool_share_percent_den,
            ).unwrap();
        initial_market_account.token_0_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position_account.token_0_fee_pool_position,
                new_pool_share_percent_num,
                new_pool_share_percent_den,
            ).unwrap();
        initial_market_account.token_1_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position_account.token_1_fee_pool_position,
                new_pool_share_percent_num,
                new_pool_share_percent_den,
            ).unwrap();
        // Redeemed lp tokens get added to the user's position.
        initial_market_account.lp_tokens_supplied_pool
            .incr_position_amt(&mut initial_liquidity_position_account.lp_token_pool_position, redeemed_lp_token_fees)
            .unwrap();

        // Assert liquidity position account changes.
        let liquidity_position_account_after = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(liquidity_position_account_after, initial_liquidity_position_account);

        // Assert market account changes.
        let market_account_after = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool, initial_market_account.lp_tokens_supplied_pool);
        assert_eq!(market_account_after.lp_token_fee_balance_pool, initial_market_account.lp_token_fee_balance_pool);
        assert_eq!(market_account_after.token_0_fee_balance_pool, initial_market_account.token_0_fee_balance_pool);
        assert_eq!(market_account_after.token_1_fee_balance_pool, initial_market_account.token_1_fee_balance_pool);
        let market_account_key = derive_market_account_pda(
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).unwrap();
        let market_raydium_lp_token_account = derive_market_token_account_pda(
            &market_account_key,
            &raydium_lp_token,
        ).unwrap();
        assert_eq!(
            tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_account).await,
            market_account_after.lp_tokens_supplied_pool.total_balance() + lp_token_fees_before_deposit + lp_token_fees_after_deposit - redeemed_lp_token_fees
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used + redeemed_token_0_fees);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used + redeemed_token_1_fees);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_partial_and_deposit_multiple_from_creator() {
        // Asserts that multiple lp position withdrawals from the creator can go through.

        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let creator = result.creator;
        let token_0 = result.token_0;
        let token_1 = result.token_1;

        let liquidity_position_account = derive_liquidity_position_account_pda(
            &derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap(),
            &creator.pubkey(),
        ).unwrap();
        token_0.mint(&mut t, &creator.pubkey(), 10000_000000000).await;
        token_1.mint(&mut t, &creator.pubkey(), 10000_000000000).await;

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            5000_000000000,
            5000_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 158113883005);
        let share_amt = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt, 60000);

        // Accrue some mock fees after the user deposits.
        let token_0_fees_before_deposit = 100;
        let token_1_fees_before_deposit = 200;
        let lp_token_fees_before_deposit = 10;
        helpers::add_mock_fees_to_market(
            &mut t,
            &token_0,
            &token_1,
            token_0_fees_before_deposit,
            token_1_fees_before_deposit,
            lp_token_fees_before_deposit,
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 250,
                min_received_lp_tokens: 790569412,
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 790569414);

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 1,
                min_received_lp_tokens: 0,
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 3162277);

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 59749,
                min_received_lp_tokens: 0,
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 188942927825);

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 100,
                min_received_lp_tokens: 0,
                burn_max: false,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 15811388);
    }

    #[tokio::test]
    async fn test_withdraw_partial_burn_max() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account_key, &raydium_lp_token).unwrap();

        let new_user = Keypair::new();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let lp_supply_before_deposit = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key)
            .await
            .lp_tokens_supplied_pool
            .total_balance();
        assert_eq!(lp_supply_before_deposit, 31622776501);

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        let liquidity_position_account = derive_liquidity_position_account_pda(&market_account_key, &new_user.pubkey()).unwrap();
        let share_amt = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt, 500);

        helpers::mock_incr_lp_token_used_for_positions(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
            lp_supply_before_deposit + lp_tokens_minted_and_deposited - 10,
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 480,
                min_received_lp_tokens: 0,
                burn_max: true,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 3162277);
        let share_amt_after = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt_after, 499);

        let expected_lp_tokens = lp_supply_before_deposit + lp_tokens_minted_and_deposited - lp_tokens_redeemed;
        let market_account_after = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool.total_balance(), expected_lp_tokens);
        assert_eq!(
        tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await,
        expected_lp_tokens,
    );

        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_partial_burn_max_not_needed() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account_key, &raydium_lp_token).unwrap();

        let new_user = Keypair::new();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;
        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        let lp_supply_before_deposit = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key)
            .await
            .lp_tokens_supplied_pool
            .total_balance();
        assert_eq!(lp_supply_before_deposit, 31622776501);

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited, 1581138830);
        let liquidity_position_account = derive_liquidity_position_account_pda(&market_account_key, &new_user.pubkey()).unwrap();
        let share_amt = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt, 500);

        helpers::mock_incr_lp_token_used_for_positions(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
            lp_supply_before_deposit,
        ).await;

        let lp_tokens_redeemed = helpers::withdraw_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 480,
                min_received_lp_tokens: 0,
                burn_max: true,
            },
        ).await.unwrap();
        assert_eq!(lp_tokens_redeemed, 1517893272);
        let share_amt_after = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt_after, 20);

        let expected_lp_tokens = lp_supply_before_deposit + lp_tokens_minted_and_deposited - lp_tokens_redeemed;
        let market_account_after = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
        assert_eq!(market_account_after.lp_tokens_supplied_pool.total_balance(), expected_lp_tokens);
        assert_eq!(
        tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await,
        expected_lp_tokens,
    );

        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance + lp_tokens_redeemed);
    }

    #[tokio::test]
    async fn test_withdraw_partial_too_many_shares() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let new_user = Keypair::new();
        let liquidity_position_account = derive_liquidity_position_account_pda(&market_account_key, &new_user.pubkey()).unwrap();
        tutils::token::get_lamports(&mut t, &new_user.pubkey(), 1000_000000000).await; // For rent payments and gas.
        tutils::token::create_token_account(&mut t, &new_user.pubkey(), &raydium_lp_token).await;

        token_0.mint(&mut t, &new_user.pubkey(), 100_000000000).await;
        token_1.mint(&mut t, &new_user.pubkey(), 100_000000000).await;

        let lp_supply_before_deposit = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key)
            .await
            .lp_tokens_supplied_pool
            .total_balance();

        let (lp_tokens_minted_and_deposited, _, _) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        let share_amt = t.get_account
            ::<limitless::state::liquidity_position::LiquidityPositionAccount>(&liquidity_position_account)
            .await
            .lp_token_pool_position.share_token_amt();
        assert_eq!(share_amt, 500);

        helpers::mock_incr_lp_token_used_for_positions(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
            lp_supply_before_deposit + lp_tokens_minted_and_deposited/2,
        ).await;

        let res = helpers::withdraw_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::WithdrawLpTokensArgs{
                share_amt: 600,
                min_received_lp_tokens: 0,
                burn_max: false,
            },
        ).await;
        tutils::assert_err(res, LimitlessError::InvalidShareAmt);
    }

}