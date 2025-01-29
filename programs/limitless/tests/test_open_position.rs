mod helpers;

#[cfg(test)]
mod tests {
    use tutils;

    use solana_program_test::{tokio};
    use solana_sdk::signature::{Signer};
    use limitless::client::accounts::{derive_market_account_pda, derive_market_token_account_pda};
    use limitless::errors::LimitlessError;
    use limitless::state::config::TradingMode;
    use limitless::state::market::QuoteToken;
    use tutils::assert_err;
    use crate::helpers;
    use crate::helpers::add_mock_fees_to_market;

    #[tokio::test]
    async fn test_open_position_long() {
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
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_token_0_ata = derive_market_token_account_pda(&market_account, &token_0.pubkey()).unwrap();
        let market_token_1_ata = derive_market_token_account_pda(&market_account, &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account, &raydium_lp_token).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Starting market and user state.
        let start_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let start_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let start_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let start_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let start_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let start_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let start_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 10_000000000,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 1050,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 38831218,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: args.collateral_quote_token_amt,
            raydium_fee_reserve_collateral_token_amt: 299030117,
            rounding_fee_reserve_collateral_token_amt: 123,
            raydium_fees_reserved_amt_quote_token: 497089262,
            blackwing_fee_reserve_quote_token_amt: 49740851,
            base_token_balance: 38831218,
            quote_token_balance: 10249761190,
            lp_tokens_removed: 620054443,
            loan_position_token_amt: 19607843,
            loan_collateral_token_amt: 19_607843132,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 961168782,
            after_open_quote_token_amt: 1000_192117252,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });

        // Assert market state.
        let end_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let end_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let end_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        assert_eq!(
            end_market_token_0_balance,
            start_market_token_0_balance
                + 38831218 // position size
        );
        assert_eq!(
            end_market_token_1_balance,
            start_market_token_1_balance
                + args.collateral_quote_token_amt // collateral
                + 49740851 // blackwing fee
                + 299030117 // raydium fees for closing
                + 123 // rounding fee
        );
        assert_eq!(
            end_market_raydium_lp_token_balance,
            start_market_raydium_lp_token_balance
                - 620054443 // lp tokens removed
        );
        let end_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(end_market_account.token_0_fee_balance_pool.total_balance(), start_market_account.token_0_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.token_1_fee_balance_pool.total_balance(), start_market_account.token_1_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_token_fee_balance_pool.total_balance(), start_market_account.lp_token_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_tokens_removed_for_positions, start_market_account.lp_tokens_removed_for_positions + 620054443);

        // Assert user state.
        let end_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let end_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let end_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(end_user_token_0_balance, start_user_token_0_balance);
        assert_eq!(
            end_user_token_1_balance,
            start_user_token_1_balance
                - args.collateral_quote_token_amt
                - 49740851 // blackwing fee
                - 299029994 // raydium fees for closing
                - 198059145 // raydium fee to open
                - 123 // rounding fee
        );
        assert_eq!(end_user_lp_token_balance, start_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_open_position_long_token_0_quote() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token0,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let creator = result.creator;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_token_0_ata = derive_market_token_account_pda(&market_account, &token_0.pubkey()).unwrap();
        let market_token_1_ata = derive_market_token_account_pda(&market_account, &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account, &raydium_lp_token).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_0.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Starting market and user state.
        let start_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let start_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let start_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let start_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let start_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let start_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let start_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 10_000000000,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 1050,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 38831218,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: args.collateral_quote_token_amt,
            raydium_fee_reserve_collateral_token_amt: 299030117,
            rounding_fee_reserve_collateral_token_amt: 123,
            raydium_fees_reserved_amt_quote_token: 497089262,
            blackwing_fee_reserve_quote_token_amt: 49740851,
            base_token_balance: 38831218,
            quote_token_balance: 10249761190,
            lp_tokens_removed: 620054443,
            loan_position_token_amt: 19607843,
            loan_collateral_token_amt: 19_607843132,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 961168782,
            after_open_quote_token_amt: 1000_192117252,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });

        // Assert market state.
        let end_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let end_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let end_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        assert_eq!(
            end_market_token_1_balance,
            start_market_token_1_balance
                + 38831218 // position size
        );
        assert_eq!(
            end_market_token_0_balance,
            start_market_token_0_balance
                + args.collateral_quote_token_amt // collateral
                + 49740851 // blackwing fee
                + 299030117 // raydium fees for closing
                + 123 // rounding fee
        );
        assert_eq!(
            end_market_raydium_lp_token_balance,
            start_market_raydium_lp_token_balance
                - 620054443 // lp tokens removed
        );
        let end_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(end_market_account.token_0_fee_balance_pool.total_balance(), start_market_account.token_0_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.token_1_fee_balance_pool.total_balance(), start_market_account.token_1_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_token_fee_balance_pool.total_balance(), start_market_account.lp_token_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_tokens_removed_for_positions, start_market_account.lp_tokens_removed_for_positions + 620054443);

        // Assert user state.
        let end_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let end_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let end_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(end_user_token_1_balance, start_user_token_1_balance);
        assert_eq!(
            end_user_token_0_balance,
            start_user_token_0_balance
                - args.collateral_quote_token_amt
                - 49740851 // blackwing fee
                - 299029994 // raydium fees for closing
                - 198059145 // raydium fee to open
                - 123 // rounding fee
        );
        assert_eq!(end_user_lp_token_balance, start_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_open_position_short() {
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
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_token_0_ata = derive_market_token_account_pda(&market_account, &token_0.pubkey()).unwrap();
        let market_token_1_ata = derive_market_token_account_pda(&market_account, &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account, &raydium_lp_token).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Starting market and user state.
        let start_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let start_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let start_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let start_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let start_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let start_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let start_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: true,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 0,
            worst_price_den: 10,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 39647400631,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: 9900990,
            raydium_fee_reserve_collateral_token_amt: 299030,
            rounding_fee_reserve_collateral_token_amt: 0,
            raydium_fees_reserved_amt_quote_token: 613460517,
            blackwing_fee_reserve_quote_token_amt: 50258316,
            base_token_balance: 10200031,
            quote_token_balance: 39697658947,
            lp_tokens_removed: 626505013,
            loan_position_token_amt: 20022037039,
            loan_collateral_token_amt: 19605823,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 989794029,
            after_open_quote_token_amt: 970_962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });

        // Assert market state.
        let end_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let end_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let end_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        assert_eq!(
            end_market_token_0_balance,
            start_market_token_0_balance
                + 9900990 // collateral
                + 299030 // raydium fees for closing
                + 11 // excess base from fee estimate
                + 0 // rounding fee
        );
        assert_eq!(
            end_market_token_1_balance,
            start_market_token_1_balance
                + 39647400631 // position size
                + 50258316 // blackwing fee
        );
        assert_eq!(
            end_market_raydium_lp_token_balance,
            start_market_raydium_lp_token_balance
                - 626505013 // lp tokens removed
        );
        let end_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(end_market_account.token_0_fee_balance_pool.total_balance(), start_market_account.token_0_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.token_1_fee_balance_pool.total_balance(), start_market_account.token_1_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_token_fee_balance_pool.total_balance(), start_market_account.lp_token_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_tokens_removed_for_positions, start_market_account.lp_tokens_removed_for_positions + 626505013);

        // Assert user state.
        let end_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let end_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let end_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(end_user_token_0_balance, start_user_token_0_balance);
        assert_eq!(
            end_user_token_1_balance,
            start_user_token_1_balance
                - args.collateral_quote_token_amt
                - 613460517 // Additional token 1 to cover token 0 open/close fees
                - 50258316 // blackwing fee
        );
        assert_eq!(end_user_lp_token_balance, start_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_open_position_short_token_0_quote() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token0,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let creator = result.creator;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_token_0_ata = derive_market_token_account_pda(&market_account, &token_0.pubkey()).unwrap();
        let market_token_1_ata = derive_market_token_account_pda(&market_account, &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account, &raydium_lp_token).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_0.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Starting market and user state.
        let start_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let start_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let start_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let start_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let start_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let start_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let start_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: true,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 0,
            worst_price_den: 10,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 39647400631,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: 9900990,
            raydium_fee_reserve_collateral_token_amt: 299030,
            rounding_fee_reserve_collateral_token_amt: 0,
            raydium_fees_reserved_amt_quote_token: 613460517,
            blackwing_fee_reserve_quote_token_amt: 50258316,
            base_token_balance: 10200031,
            quote_token_balance: 39697658947,
            lp_tokens_removed: 626505013,
            loan_position_token_amt: 20022037039,
            loan_collateral_token_amt: 19605823,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 989794029,
            after_open_quote_token_amt: 970_962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });

        // Assert market state.
        let end_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let end_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let end_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        assert_eq!(
            end_market_token_1_balance,
            start_market_token_1_balance
                + 9900990 // collateral
                + 299030 // raydium fees for closing
                + 11 // excess token 1 from fee estimate
                + 0 // rounding fee
        );
        assert_eq!(
            end_market_token_0_balance,
            start_market_token_0_balance
                + 39647400631 // position size
                + 50258316 // blackwing fee
        );
        assert_eq!(
            end_market_raydium_lp_token_balance,
            start_market_raydium_lp_token_balance
                - 626505013 // lp tokens removed
        );
        let end_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(end_market_account.token_0_fee_balance_pool.total_balance(), start_market_account.token_0_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.token_1_fee_balance_pool.total_balance(), start_market_account.token_1_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_token_fee_balance_pool.total_balance(), start_market_account.lp_token_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_tokens_removed_for_positions, start_market_account.lp_tokens_removed_for_positions + 626505013);

        // Assert user state.
        let end_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let end_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let end_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(end_user_token_1_balance, start_user_token_1_balance);
        assert_eq!(
            end_user_token_0_balance,
            start_user_token_0_balance
                - args.collateral_quote_token_amt
                - 613460517 // Additional token 0 to cover token 1 open/close fees
                - 50258316 // blackwing fee
        );
        assert_eq!(end_user_lp_token_balance, start_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_open_position_fees_deposited() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token0,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let creator = result.creator;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_token_0_ata = derive_market_token_account_pda(&market_account, &token_0.pubkey()).unwrap();
        let market_token_1_ata = derive_market_token_account_pda(&market_account, &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account, &raydium_lp_token).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_0.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Mock token_0 and token_1 fees.
        add_mock_fees_to_market(
            &mut t,
            &token_0,
            &token_1,
            1000000000,
            1000000000,
            0,
        ).await;

        // Starting market and user state.
        let start_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let start_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let start_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let start_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let start_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let start_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let start_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 0,
            worst_price_den: 10,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 39212233,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: 10000000000,
            raydium_fee_reserve_collateral_token_amt: 302019369,
            rounding_fee_reserve_collateral_token_amt: 0,
            raydium_fees_reserved_amt_quote_token: 502039371,
            blackwing_fee_reserve_quote_token_amt: 50233334,
            base_token_balance: 39212233,
            quote_token_balance: 10352252703,
            lp_tokens_removed: 626193596,
            loan_position_token_amt: 19801980,
            loan_collateral_token_amt: 19801980196,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_001000000,
            open_quote_token_amt: 1000_999999981,
            after_open_base_token_amt: 961787767,
            after_open_quote_token_amt: 1001194019383,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });

        // Assert market state.
        let end_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let end_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let end_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        assert_eq!(
            end_market_token_0_balance,
            start_market_token_0_balance
                + 10000000000 // collateral
                + 302019369 // raydium fees for closing
                + 0 // rounding fee
                + 50233334 // blackwing fee
                - 999999981 // token 0 deposited to mint LP
        );
        assert_eq!(
            end_market_token_1_balance,
            start_market_token_1_balance
                + 39212233 // position size
                - 1000000 // token 1 deposited to mint LP
        );
        assert_eq!(
            end_market_raydium_lp_token_balance,
            start_market_raydium_lp_token_balance
                - 626193596 // lp tokens removed
                + 31622776 // lp token fees minted
        );
        let end_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(
            end_market_account.token_0_fee_balance_pool.total_balance(),
            start_market_account.token_0_fee_balance_pool.total_balance()
                - 999999981 // token 0 deposited to mint LP,
        );
        assert_eq!(
            end_market_account.token_1_fee_balance_pool.total_balance(),
            start_market_account.token_1_fee_balance_pool.total_balance()
                - 1000000 // token 1 deposited to mint LP,
        );
        assert_eq!(
            end_market_account.lp_token_fee_balance_pool.total_balance(),
            start_market_account.lp_token_fee_balance_pool.total_balance()
                + 31622776 // lp token fees minted,
        );
        assert_eq!(end_market_account.lp_tokens_removed_for_positions, start_market_account.lp_tokens_removed_for_positions + 626193596);

        // Assert user state.
        let end_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let end_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let end_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(end_user_token_1_balance, start_user_token_1_balance);
        assert_eq!(
            end_user_token_0_balance,
            start_user_token_0_balance
                - args.collateral_quote_token_amt
                - 502039371 // raydium fees
                - 50233334 // blackwing fee
        );
        assert_eq!(end_user_lp_token_balance, start_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_open_position_fees_deposited_with_swap() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token0,
            fee_collector_override: None,
            quote_token_wsol: false,
        }).await;
        let creator = result.creator;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_token_0_ata = derive_market_token_account_pda(&market_account, &token_0.pubkey()).unwrap();
        let market_token_1_ata = derive_market_token_account_pda(&market_account, &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account, &raydium_lp_token).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_0.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Mock token_0 and token_1 fees.
        add_mock_fees_to_market(
            &mut t,
            &token_0,
            &token_1,
            1000000000,
            0,
            0,
        ).await;

        // Starting market and user state.
        let start_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let start_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let start_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let start_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 0,
            worst_price_den: 10,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 39173835,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: 10000000000,
            raydium_fee_reserve_collateral_token_amt: 302019878,
            rounding_fee_reserve_collateral_token_amt: 0,
            raydium_fees_reserved_amt_quote_token: 502040860,
            blackwing_fee_reserve_quote_token_amt: 50233580,
            base_token_balance: 39173835,
            quote_token_balance: 10352253458,
            lp_tokens_removed: 625887017,
            loan_position_token_amt: 19782593,
            loan_collateral_token_amt: 19802077136,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1000000000,
            open_quote_token_amt: 1000984899351,
            after_open_base_token_amt: 960826165,
            after_open_quote_token_amt: 1001178919705,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });

        // Assert market state.
        let end_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let end_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let end_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        assert_eq!(
            end_market_token_0_balance,
            start_market_token_0_balance
                + 10000000000 // collateral
                + 302019878 // raydium fees for closing
                + 50233580 // blackwing fee
                - 494877548 // token 0 swapped to mint LP
                - 490170265 // token 0 deposited to mint LP
        );
        assert_eq!(
            end_market_token_1_balance,
            start_market_token_1_balance
                + 39173835 // position size
                + 489688 // token 1 received after swap to deposit as LP
                - 489688 // token 1 deposited to mint LP
        );
        assert_eq!(
            end_market_raydium_lp_token_balance,
            start_market_raydium_lp_token_balance
                - 625887017 // lp tokens removed
                + 15492880 // lp token fees minted
        );
        let end_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(
            end_market_account.token_0_fee_balance_pool.total_balance(),
            start_market_account.token_0_fee_balance_pool.total_balance()
                - 494877548 // token 0 swapped to mint LP
                - 490170265 // token 0 deposited to mint LP
        );
        assert_eq!(
            end_market_account.token_1_fee_balance_pool.total_balance(),
            start_market_account.token_1_fee_balance_pool.total_balance()
                + 489688 // token 1 received after swap to deposit as LP
                - 489688 // token 1 deposited to mint LP
        );
        assert_eq!(
            end_market_account.lp_token_fee_balance_pool.total_balance(),
            start_market_account.lp_token_fee_balance_pool.total_balance()
                + 15492880 // lp token fees minted,
        );
        assert_eq!(
            end_market_account.lp_tokens_removed_for_positions,
            start_market_account.lp_tokens_removed_for_positions
                + 625887017 // lp tokens borrowed
        );
    }

    #[tokio::test]
    async fn test_open_position_max_blackwing_fee() {
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

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 49740850,
            rollover_reserve_quote_token_amt: 20_000000000,
            worst_price_num: 1050,
            worst_price_den: 1,
        };
        let res = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await;

        assert_err(res, LimitlessError::MaxBlackwingFeeExceeded);
    }

    #[tokio::test]
    async fn test_open_position_max_raydium_fee() {
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

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 100000,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 20_000000000,
            worst_price_num: 1050,
            worst_price_den: 1,
        };
        let res = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await;

        assert_err(res, LimitlessError::MaxRaydiumFeeExceeded);
    }

    #[tokio::test]
    async fn test_open_position_with_rollover_fee_reserve() {
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
        let raydium_lp_token = utils::raydium::pda::lp_mint_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
        let market_token_0_ata = derive_market_token_account_pda(&market_account, &token_0.pubkey()).unwrap();
        let market_token_1_ata = derive_market_token_account_pda(&market_account, &token_1.pubkey()).unwrap();
        let market_raydium_lp_token_ata = derive_market_token_account_pda(&market_account, &raydium_lp_token).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Starting market and user state.
        let start_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let start_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let start_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let start_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let start_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let start_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let start_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 10_000000000,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 20_000000000,
            worst_price_num: 1050,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 38831218,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: args.collateral_quote_token_amt,
            raydium_fee_reserve_collateral_token_amt: 299030117,
            rounding_fee_reserve_collateral_token_amt: 123,
            raydium_fees_reserved_amt_quote_token: 497089262,
            blackwing_fee_reserve_quote_token_amt: 49740851,
            base_token_balance: 38831218,
            quote_token_balance: 10249761190+20_000000000,
            lp_tokens_removed: 620054443,
            loan_position_token_amt: 19607843,
            loan_collateral_token_amt: 19_607843132,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 961168782,
            after_open_quote_token_amt: 1000_192117252,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });

        // Assert market state.
        let end_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let end_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let end_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        assert_eq!(
            end_market_token_0_balance,
            start_market_token_0_balance
                + 38831218 // position size
        );
        assert_eq!(
            end_market_token_1_balance,
            start_market_token_1_balance
                + args.collateral_quote_token_amt // collateral
                + 49740851 // blackwing fee
                + 299030117 // raydium fees for closing
                + 123 // rounding fee
                + 20_000000000 // rollover reserve
        );
        assert_eq!(
            end_market_raydium_lp_token_balance,
            start_market_raydium_lp_token_balance
                - 620054443 // lp tokens removed
        );
        let end_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(end_market_account.token_0_fee_balance_pool.total_balance(), start_market_account.token_0_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.token_1_fee_balance_pool.total_balance(), start_market_account.token_1_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_token_fee_balance_pool.total_balance(), start_market_account.lp_token_fee_balance_pool.total_balance());
        assert_eq!(end_market_account.lp_tokens_removed_for_positions, start_market_account.lp_tokens_removed_for_positions + 620054443);

        // Assert user state.
        let end_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let end_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let end_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(end_user_token_0_balance, start_user_token_0_balance);
        assert_eq!(
            end_user_token_1_balance,
            start_user_token_1_balance
                - args.collateral_quote_token_amt
                - 49740851 // blackwing fee
                - 299029994 // raydium fees for closing
                - 198059145 // raydium fee to open
                - 123 // rounding fee
                - 20_000000000 // rollover reserve
        );
        assert_eq!(end_user_lp_token_balance, start_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_open_position_long_slippage() {
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

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 1001,
            worst_price_den: 1,
        };
        let res = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await;
        assert_err(res, LimitlessError::SlippageExceeded);
    }

    #[tokio::test]
    async fn test_open_position_short_slippage() {
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

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: true,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 990,
            worst_price_den: 1,
        };
        let res = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await;
        assert_err(res, LimitlessError::SlippageExceeded);
    }

    #[tokio::test]
    async fn test_open_position_trading_mode_not_enabled() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
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

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        // Market disabled.
        {
            let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
            let mut market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
            market_account.trading_mode = TradingMode::Disabled;
            t.write_account(&market_account_key, &market_account).await;

            let res = helpers::open_position(
                &mut t,
                &creator,
                &token_0.pubkey(),
                &token_1.pubkey(),
                limitless::instructions::OpenPositionArgs{
                    id: uuid::Uuid::new_v4(),
                    collateral_quote_token_amt: 10_000000000,
                    is_short: true,
                    duration_blocks: 1000000,
                    max_raydium_fee_quote_token_amt: 0,
                    max_blackwing_fee_quote_token_amt: 0,
                    rollover_reserve_quote_token_amt: 0,
                    worst_price_num: 990,
                    worst_price_den: 1,
                },
            ).await;
            assert_err(res, LimitlessError::MarketClosed);
        }

        // Market position closing only.
        {
            let market_account_key = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();
            let mut market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account_key).await;
            market_account.trading_mode = TradingMode::PositionCloseOnly;
            t.write_account(&market_account_key, &market_account).await;

            let res = helpers::open_position(
                &mut t,
                &creator,
                &token_0.pubkey(),
                &token_1.pubkey(),
                limitless::instructions::OpenPositionArgs{
                    id: uuid::Uuid::new_v4(),
                    collateral_quote_token_amt: 10_000000000,
                    is_short: true,
                    duration_blocks: 1000000,
                    max_raydium_fee_quote_token_amt: 0,
                    max_blackwing_fee_quote_token_amt: 0,
                    rollover_reserve_quote_token_amt: 0,
                    worst_price_num: 990,
                    worst_price_den: 1,
                },
            ).await;
            assert_err(res, LimitlessError::MarketClosed);
        }
    }

    #[tokio::test]
    async fn test_open_position_long_not_enough_user_funds() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
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

        // Give the user money but not enough to cover the fees.
        token_1.mint(&mut t, &creator.pubkey(), 10_000000000).await;

        let res = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::OpenPositionArgs{
                id: uuid::Uuid::new_v4(),
                collateral_quote_token_amt: 10_000000000,
                is_short: false,
                duration_blocks: 1000000,
                max_raydium_fee_quote_token_amt: 0,
                max_blackwing_fee_quote_token_amt: 0,
                rollover_reserve_quote_token_amt: 0,
                worst_price_num: 990,
                worst_price_den: 1,
            },
        ).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_open_position_short_not_enough_user_funds() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 100_000000000,
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

        // Give the user money but not enough to cover the collateral.
        token_1.mint(&mut t, &creator.pubkey(), 1_000000000).await;

        let res = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::OpenPositionArgs{
                id: uuid::Uuid::new_v4(),
                collateral_quote_token_amt: 10_000000000,
                is_short: true,
                duration_blocks: 1000000,
                max_raydium_fee_quote_token_amt: 0,
                max_blackwing_fee_quote_token_amt: 0,
                rollover_reserve_quote_token_amt: 0,
                worst_price_num: 990,
                worst_price_den: 1,
            },
        ).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_open_position_existing_position_id() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
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

        // Give the user money but not enough to cover the fees.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        let pos_id = uuid::Uuid::new_v4();

        helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::OpenPositionArgs{
                id: pos_id.clone(),
                collateral_quote_token_amt: 500000000,
                is_short: true,
                duration_blocks: 1000000,
                max_raydium_fee_quote_token_amt: 0,
                max_blackwing_fee_quote_token_amt: 0,
                rollover_reserve_quote_token_amt: 0,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let res = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::OpenPositionArgs{
                id: pos_id,
                collateral_quote_token_amt: 100000000,
                is_short: true,
                duration_blocks: 1000000,
                max_raydium_fee_quote_token_amt: 0,
                max_blackwing_fee_quote_token_amt: 0,
                rollover_reserve_quote_token_amt: 0,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::PositionIdAlreadyUsed);
    }

    #[tokio::test]
    async fn test_open_position_long_wsol_quote() {
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: None,
            quote_token_wsol: true,
        }).await;
        let creator = result.creator;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        // Give the user money.
        token_1.mint(&mut t, &creator.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 10_000000000,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 1050,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        // Assert position state.
        assert_eq!(pos, limitless::state::position::PositionAccount{
            id: args.id,
            market_account: market_account.clone(),
            position_size: 38831218,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: args.collateral_quote_token_amt,
            raydium_fee_reserve_collateral_token_amt: 299030117,
            rounding_fee_reserve_collateral_token_amt: 123,
            raydium_fees_reserved_amt_quote_token: 497089262,
            blackwing_fee_reserve_quote_token_amt: 49740851,
            base_token_balance: 38831218,
            quote_token_balance: 10249761190,
            lp_tokens_removed: 620054443,
            loan_position_token_amt: 19607843,
            loan_collateral_token_amt: 19_607843132,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 961168782,
            after_open_quote_token_amt: 1000_192117252,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        });
    }
}
