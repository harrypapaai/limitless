mod helpers;

use solana_program_test::tokio;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use limitless::client::accounts::derive_market_account_pda;
use limitless::state::config::TradingMode;
use limitless::state::market::QuoteToken;
use tutils::assert_err;

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_update_market_config() {
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
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        let old_market = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;

        let args = limitless::instructions::UpdateMarketConfigArgs{
            trading_mode: Some(TradingMode::Disabled),
            base_fee_apr: Some(1000000),
            min_fee_quote_token: None,
            min_duration_slots: None,
            max_duration_slots: None,
        };
        helpers::update_market_config(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        let market = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;

        // Assert market state.
        assert_eq!(market, limitless::state::market::MarketAccount{
            trading_mode: TradingMode::Disabled,
            quote_token: QuoteToken::Token1,
            base_fee_apr: 1000000,
            min_fee_quote_token: old_market.min_fee_quote_token,
            min_duration_slots: old_market.min_duration_slots,
            max_duration_slots: old_market.max_duration_slots,
            lp_tokens_supplied_pool: old_market.lp_tokens_supplied_pool,
            lp_tokens_removed_for_positions: old_market.lp_tokens_removed_for_positions,
            lp_token_fee_balance_pool: old_market.lp_token_fee_balance_pool,
            token_0_fee_balance_pool: old_market.token_0_fee_balance_pool,
            token_1_fee_balance_pool: old_market.token_1_fee_balance_pool,
            creator: creator.pubkey(),
            space: [0; 128],
        });
    }

    #[tokio::test]
    async fn test_update_market_config_invalid_admin() {
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

        let random_user = Keypair::new();
        let args = limitless::instructions::UpdateMarketConfigArgs{
            trading_mode: Some(TradingMode::Disabled),
            base_fee_apr: Some(1000000),
            min_fee_quote_token: None,
            min_duration_slots: None,
            max_duration_slots: None,
        };
        let res = helpers::update_market_config(
            &mut t,
            &random_user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await;
        assert_err(res, limitless::errors::LimitlessError::InvalidAdmin);
    }
}