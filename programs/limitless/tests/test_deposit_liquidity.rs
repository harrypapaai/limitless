mod helpers;

use solana_program_test::tokio;
use solana_sdk::signature::{Keypair, Signer};
use limitless::client::accounts::{derive_market_account_pda, derive_market_token_account_pda};
use limitless::state::market::QuoteToken;

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_deposit_lp_tokens_new_position() {
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
        let lp_token_fees = 5;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees, token_1_fees, lp_token_fees).await;

        // Get the initial pool states.
        let (
            initial_lp_token_pool,
            initial_lp_token_fee_pool,
            initial_token_0_fee_pool,
            initial_token_1_fee_pool,
        ) = {
            let market_account = helpers::get_market_account(
                &mut t,
                &token_0.pubkey(),
                &token_1.pubkey(),
            ).await;
            (
                market_account.lp_tokens_supplied_pool.clone(),
                market_account.lp_token_fee_balance_pool.clone(),
                market_account.token_0_fee_balance_pool.clone(),
                market_account.token_1_fee_balance_pool.clone(),
            )
        };
        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        // Actually deposit assets.
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

        let expected_lp_token_shares = initial_lp_token_fee_pool
            .clone()
            .mint_position(50000000, 1_000000000)
            .unwrap()
            .share_token_amt();
        let expected_lp_token_fee_shares = initial_lp_token_fee_pool
            .clone()
            .mint_position(50000000, 1_000000000)
            .unwrap()
            .share_token_amt();
        let expected_token_0_fee_shares = initial_token_0_fee_pool
            .clone()
            .mint_position(50000000, 1_000000000)
            .unwrap()
            .share_token_amt();
        let expected_token_1_fee_shares = initial_token_1_fee_pool
            .clone()
            .mint_position(50000000, 1_000000000)
            .unwrap()
            .share_token_amt();

        // Assert liquidity position account changes.
        let liquidity_position_account = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(liquidity_position_account.lp_token_pool_position.share_token_amt(), expected_lp_token_shares);
        assert_eq!(liquidity_position_account.lp_token_fee_pool_position.share_token_amt(), expected_lp_token_fee_shares);
        assert_eq!(liquidity_position_account.token_0_fee_pool_position.share_token_amt(), expected_token_0_fee_shares);
        assert_eq!(liquidity_position_account.token_1_fee_pool_position.share_token_amt(), expected_token_1_fee_shares);

        // Assert market account changes.
        let market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(
        market_account.lp_tokens_supplied_pool.total_balance(),
        initial_lp_token_pool.total_balance()+lp_tokens_minted_and_deposited,
    );
        assert_eq!(
        market_account.lp_tokens_supplied_pool.share_token_supply(),
        initial_lp_token_pool.share_token_supply()+expected_lp_token_shares,
    );
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
            market_account.lp_tokens_supplied_pool.total_balance() + lp_token_fees
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_deposit_lp_tokens_existing_position() {
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

        let initial_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let initial_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let initial_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;

        // Create initial LP position.
        let (lp_tokens_minted_and_deposited_first_deposit, token_0_used_first_deposit, token_1_used_first_deposit) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            50_000000000,
            50_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited_first_deposit, 1581138830);
        assert_eq!(token_0_used_first_deposit, 50000000);
        assert_eq!(token_1_used_first_deposit, 49_999999999);

        // Get state of pools and position.
        let mut initial_liquidity_position = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        let mut initial_market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;

        // Accrue some mock fees.
        let token_0_fees = 100;
        let token_1_fees = 200;
        let lp_token_fees = 5;
        helpers::add_mock_fees_to_market(&mut t, &token_0, &token_1, token_0_fees, token_1_fees, lp_token_fees).await;
        initial_market_account.lp_token_fee_balance_pool.incr_balance(lp_token_fees).unwrap();
        initial_market_account.token_0_fee_balance_pool.incr_balance(token_0_fees).unwrap();
        initial_market_account.token_1_fee_balance_pool.incr_balance(token_1_fees).unwrap();

        // Deposit more LP.
        let lp_token_balance_before_deposit = initial_market_account.lp_tokens_supplied_pool.total_balance();
        let (lp_tokens_minted_and_deposited_second_deposit, token_0_used_second_deposit, token_1_used_second_deposit) = helpers::deposit_liquidity(
            &mut t,
            &new_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            20_000000000,
            10_000000000,
        ).await;
        assert_eq!(lp_tokens_minted_and_deposited_second_deposit, 316227766);
        assert_eq!(token_0_used_second_deposit, 10000000);
        assert_eq!(token_1_used_second_deposit, 10000000000);

        // Assert state of liquidity position.

        // Any accrued fees should be redeemed.
        let lp_token_fees_redeemed = initial_market_account.lp_token_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position.lp_token_fee_pool_position)
            .unwrap();
        let token_0_fees_redeemed = initial_market_account.token_0_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position.token_0_fee_pool_position)
            .unwrap();
        let token_1_fees_redeemed = initial_market_account.token_1_fee_balance_pool
            .redeem_position(&mut initial_liquidity_position.token_1_fee_pool_position)
            .unwrap();
        assert_eq!(lp_token_fees_redeemed, 0); // Not enough share of pool and LP fees to make up 1 token.
        assert_eq!(token_0_fees_redeemed, 4);
        assert_eq!(token_1_fees_redeemed, 9);

        initial_market_account.lp_tokens_supplied_pool
            .incr_position_amt(
                &mut initial_liquidity_position.lp_token_pool_position,
                lp_tokens_minted_and_deposited_second_deposit,
            )
            .unwrap();
        let expected_lp_token_shares = initial_liquidity_position.lp_token_pool_position.share_token_amt();

        initial_market_account.lp_token_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position.lp_token_fee_pool_position,
                316227766,
                lp_token_balance_before_deposit,
            )
            .unwrap();
        let expected_lp_token_fee_shares = initial_liquidity_position.lp_token_fee_pool_position.share_token_amt();

        initial_market_account.token_0_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position.token_0_fee_pool_position,
                316227766,
                lp_token_balance_before_deposit,
            )
            .unwrap();
        let expected_token_0_fee_shares = initial_liquidity_position.token_0_fee_pool_position.share_token_amt();

        initial_market_account.token_1_fee_balance_pool
            .incr_position_share(
                &mut initial_liquidity_position.token_1_fee_pool_position,
                316227766,
                lp_token_balance_before_deposit,
            )
            .unwrap();
        let expected_token_1_fee_shares = initial_liquidity_position.token_1_fee_pool_position.share_token_amt();

        let liquidity_position_after = helpers::get_liquidity_position_account(
            &mut t,
            &new_user.pubkey(),
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(liquidity_position_after.lp_token_pool_position.share_token_amt(), expected_lp_token_shares);
        assert_eq!(liquidity_position_after.lp_token_fee_pool_position.share_token_amt(), expected_lp_token_fee_shares);
        assert_eq!(liquidity_position_after.token_0_fee_pool_position.share_token_amt(), expected_token_0_fee_shares);
        assert_eq!(liquidity_position_after.token_1_fee_pool_position.share_token_amt(), expected_token_1_fee_shares);

        // Assert market account changes.
        let market_account = helpers::get_market_account(
            &mut t,
            &token_0.pubkey(),
            &token_1.pubkey(),
        ).await;
        assert_eq!(
        market_account.lp_tokens_supplied_pool.total_balance(),
        initial_market_account.lp_tokens_supplied_pool.total_balance(),
    );
        assert_eq!(
        market_account.lp_tokens_supplied_pool.share_token_supply(),
        initial_market_account.lp_tokens_supplied_pool.share_token_supply(),
    );
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
            market_account.lp_tokens_supplied_pool.total_balance() + lp_token_fees
        );

        // Assert user token balances.
        let user_token_0_balance_after = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &new_user.pubkey()).await;
        let user_token_1_balance_after = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &new_user.pubkey()).await;
        let user_lp_token_balance_after = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &new_user.pubkey()).await;
        assert_eq!(user_token_0_balance_after, initial_user_token_0_balance - token_0_used_first_deposit - token_0_used_second_deposit + token_0_fees_redeemed);
        assert_eq!(user_token_1_balance_after, initial_user_token_1_balance - token_1_used_first_deposit - token_1_used_second_deposit + token_1_fees_redeemed);
        assert_eq!(user_lp_token_balance_after, initial_user_lp_token_balance);
    }
}
