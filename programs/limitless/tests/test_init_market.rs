mod helpers;

use solana_program_test::{processor, tokio};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use limitless::client::accounts::derive_market_account_pda;
use limitless::pool::{BalancePool, Pool};
use limitless::state::config::TradingMode;
use crate::helpers::admin_init_limitless;

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_init_market() {
        let t = &mut tutils::new_t()
            .with_raydium(None)
            .with_program(
                "limitless",
                limitless::ID,
                processor!(limitless::entrypoint::process_instruction),
            )
            .build().await;
        let (token_0, token_1) = tutils::token::create_mock_token_pair(t).await;

        let creator = Keypair::new();

        // Init limitless.
        admin_init_limitless(t, &creator.pubkey()).await;

        tutils::token::mint_mock_token(t, &token_0, &creator.pubkey(), 1_000000000).await;
        tutils::token::mint_mock_token(t, &token_1, &creator.pubkey(), 100000_000000000).await;
        tutils::raydium::exec_init_market(
            t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            1_000000000,
            100000_000000000,
        ).await;

        helpers::init_market(
            t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            &creator.pubkey(), // fee_collector
            limitless::instructions::InitMarketArgs {
                trading_mode: TradingMode::PositionCloseOnly,
                base_fee_apr: 100000,
                min_fee_quote_token: 1000,
                min_duration_slots: 10,
                max_duration_slots: 1000,
                quote_token: limitless::state::market::QuoteToken::Token0,
            },
        ).await;

        // Validate the account was created successfully.
        let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = t.get_account::<limitless::state::market::MarketAccount>(
            &market_account_key,
        ).await;
        assert_eq!(market_account, limitless::state::market::MarketAccount {
            trading_mode: TradingMode::PositionCloseOnly,
            quote_token: limitless::state::market::QuoteToken::Token0,
            base_fee_apr: 100000,
            min_fee_quote_token: 1000,
            min_duration_slots: 10,
            max_duration_slots: 1000,

            lp_tokens_supplied_pool: Pool::new(),
            lp_tokens_removed_for_positions: 0,
            lp_token_fee_balance_pool: BalancePool::new(),

            token_0_fee_balance_pool: BalancePool::new(),
            token_1_fee_balance_pool: BalancePool::new(),

            creator: creator.pubkey(),

            space: [0; 128],
        })
    }

}