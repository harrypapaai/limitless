mod helpers;

#[cfg(test)]
mod test {
    use solana_program::pubkey::Pubkey;
    use solana_program_test::tokio;
    use solana_sdk::signature::{Keypair, Signer};
    use limitless::client::accounts::{derive_market_account_pda, derive_market_token_account_pda, derive_position_account_pda};
    use limitless::errors::LimitlessError;
    use limitless::state::market::QuoteToken;
    use tutils::{assert_err, load_limitless_closer_keypair};
    use crate::helpers;
    use crate::helpers::{add_mock_fees_to_market, get_fee_collector_quote_token_balance};

    #[tokio::test]
    async fn test_close_position_long_in_profit() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                + 39792681, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                + 1_225848332, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_long_in_profit_token_0_quote() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token0,
            fee_collector_override: Some(fee_collector_override),
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token0, 1100, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance - 38831218 // position token 1 balance,
        );
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance
                - 10249761190 // position token 0 balance
                + 39792681, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_1_balance, after_open_user_token_1_balance);
        assert_eq!(
            after_close_user_token_0_balance,
            after_open_user_token_0_balance
                + args.collateral_quote_token_amt
                + 1_225848332, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_long_in_loss() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 800, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                + 39792681, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 543595024 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 543595024 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                - 4_019897936, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_short_in_profit() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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
            worst_price_num: 900,
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
            after_open_quote_token_amt: 970962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 900, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 10200031 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 39697658947 // position token 1 balance
                + 40206653, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 10051663
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 600003406 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 626505013 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 626505013 // position lp tokens
                + 600003406 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 40206653,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                + 1_040997723, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_short_in_profit_token_0_quote() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1000_000000000,
            pool_token_1_amt: 1_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token0,
            fee_collector_override: Some(fee_collector_override),
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
            worst_price_num: 900,
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
            after_open_quote_token_amt: 970962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token0, 900, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance - 10200031 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance
                - 39697658947 // position token 1 balance
                + 40206653, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 10051663
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 600003406 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 626505013 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 626505013 // position lp tokens
                + 600003406 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance() + 40206653,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_1_balance, after_open_user_token_1_balance);
        assert_eq!(
            after_close_user_token_0_balance,
            after_open_user_token_0_balance
                + args.collateral_quote_token_amt
                + 1_040997723, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_short_in_loss() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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
            worst_price_num: 900,
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
            after_open_quote_token_amt: 970962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1200, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 10200031 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 39697658947 // position token 1 balance
                + 40206653, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 10051663
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 566374408 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 626505013 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 626505013 // position lp tokens
                + 566374408 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 40206653,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                - 1_694421892, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_long_before_duration_expires() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                + 3980 // LP fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 995
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 3980,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                + 1_275584208, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_short_before_duration_expires() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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
            worst_price_num: 900,
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
            after_open_quote_token_amt: 970962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1200, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 10200031 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 39697658947 // position token 1 balance
                + 4021, // LP fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 1005
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 566374408 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 626505013 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 626505013 // position lp tokens
                + 566374408 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 4021,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                - 1_644168602, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_fees_deposited() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        // Mock token_0 and token_1 fees.
        add_mock_fees_to_market(
            &mut t,
            &token_0,
            &token_1,
            1000000000,
            1000000000,
            0,
        ).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance
                - 38831218 // position token 0 balance
                - 909091 // token 0 fees deposited
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                + 39792681 // blackwing fees
                - 999999991 // token 1 fees deposited
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance
                + 602993779 // expected lp tokens minted on repayment
                + 29322938 // lp tokens received from fee deposit
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993779 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance()
                + 29322938 // lp tokens received from fee deposit
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance()
                - 909091 // token 0 fees deposited,
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance()
                + 39792681 // fees from trade
                - 999999991 // token 1 fees deposited
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                + 1_226244153, // pnl
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_fees_deposited_with_swap() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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
            position_size: 39211841,
            user_quote_token_collateral_amt: args.collateral_quote_token_amt,
            collateral_amt: 10000000000,
            raydium_fee_reserve_collateral_token_amt: 302020358,
            rounding_fee_reserve_collateral_token_amt: 148,
            raydium_fees_reserved_amt_quote_token: 502040508,
            blackwing_fee_reserve_quote_token_amt: 50233334,
            base_token_balance: 39211841,
            quote_token_balance: 10352253840,
            lp_tokens_removed: 626193596,
            loan_position_token_amt: 19801980,
            loan_collateral_token_amt: 19801980196,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 960788159,
            after_open_quote_token_amt: 1000194019402,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1200, 1).await;

        // Mock token_0 and token_1 fees.
        add_mock_fees_to_market(
            &mut t,
            &token_0,
            &token_1,
            0,
            10_000000000,
            0,
        ).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance
                - 39211841 // position token 0 balance
                + 4057810 // token 0 received from token 1 swap fees
                - 4057810 // token 0 fees deposited as LP
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10352253840 // position token 1 balance
                + 50233334 // blackwing fees
                - 10046666 // protocol portion of fees
                - 4_939419336 // token 1 fees swapped into token 0 to deposit as LP
                - 4_910968025 // token 1 fees deposited as LP
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 10046666
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance
                + 583141874 // expected lp tokens minted on repayment
                + 131466765 // lp tokens minted as fees
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 626193596 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 626193596 // position lp tokens
                + 583141874 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance()
                + 131466765 // lp tokens fees minted
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance()
                + 4057810 // token 0 received from token 1 swap fees
                - 4057810 // token 0 fees deposited as LP
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance()
                - 4_939419336 // token 1 fees swapped into token 0 to deposit as LP
                - 4_910968025 // token 1 fees deposited as LP
                + 50233334 // blackwing fees
                - 10046666 // protocol portion of fees
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                + 3_399619070, // pnl

        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_from_external_before_duration_expired() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

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
            worst_price_num: 900,
            worst_price_den: 1,
        };
        helpers::open_position(
            &mut t,
            &user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        t.set_slot(1000050).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        tutils::token::get_lamports(&mut t, &closer.pubkey(), 100_000000000).await;
        let res = helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::InvalidSigner);
    }

    #[tokio::test]
    async fn test_close_position_external_after_duration_expired() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
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

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

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
            &user,
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 800, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        tutils::token::get_lamports(&mut t, &closer.pubkey(), 100_000000000).await;
        helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                + 39792681, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 543595024 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 543595024 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                - 4_019897936, // pnl
        );
    }

    #[tokio::test]
    async fn test_close_position_external_invalid_closer() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

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
            worst_price_num: 1050,
            worst_price_den: 1,
        };
        helpers::open_position(
            &mut t,
            &user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 800, 1).await;
        t.set_slot(1000200).await;

        // Close position as invalid closer
        let closer = Keypair::new();
        tutils::token::get_lamports(&mut t, &user.pubkey(), 100_000000000).await;
        let res = helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::InvalidCloser);

        // close as valid closer
        let closer = load_limitless_closer_keypair();
        tutils::token::get_lamports(&mut t, &closer.pubkey(), 100_000000000).await;
        helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &user.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_close_position_doesnt_exist() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
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
            collateral_quote_token_amt: 10_000000000,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
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

        t.set_slot(1000200).await;

        let random_id = uuid::Uuid::new_v4();

        let res = helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: random_id.clone(),
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::PositionDoesNotExist);

        let closer = load_limitless_closer_keypair();
        tutils::token::get_lamports(&mut t, &closer.pubkey(), 100_000000000).await;
        let res = helpers::close_position_as_external(
            &mut t,
            &creator.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: random_id.clone(),
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::PositionDoesNotExist);

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert_eq!(
            t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
            pos,
        );
    }

    #[tokio::test]
    async fn test_close_position_external_rollover() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
        }).await;
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

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: true,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 20_000000000,
            worst_price_num: 900,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &user,
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
            quote_token_balance: 39697658947+20_000000000,
            lp_tokens_removed: 626505013,
            loan_position_token_amt: 20022037039,
            loan_collateral_token_amt: 19605823,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 989794029,
            after_open_quote_token_amt: 970962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1200, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        tutils::token::get_lamports(&mut t, &closer.pubkey(), 100_000000000).await;
        helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &user.pubkey(), args.id).unwrap();
        assert_eq!(
            t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
            limitless::state::position::PositionAccount{
                id: args.id,
                market_account: market_account.clone(),
                position_size: 39647400631,
                user_quote_token_collateral_amt: args.collateral_quote_token_amt,
                collateral_amt: 9900990,
                raydium_fee_reserve_collateral_token_amt: 299030,
                rounding_fee_reserve_collateral_token_amt: 0,
                raydium_fees_reserved_amt_quote_token: 613460517,
                blackwing_fee_reserve_quote_token_amt: 60901018,
                base_token_balance: 10200031,
                quote_token_balance: 39697658947+20_000000000-50258316,
                lp_tokens_removed: 626505013,
                loan_position_token_amt: 20022037039,
                loan_collateral_token_amt: 19605823,
                is_short: args.is_short,
                open_block: 100,
                close_block: 1000200+pos.rollover_duration_blocks,
                open_base_token_amt: 1_000000000,
                open_quote_token_amt: 1000_000000000,
                after_open_base_token_amt: 989794029,
                after_open_quote_token_amt: 970962875848,
                rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
                rollover_duration_blocks: args.duration_blocks,
                rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt-60901018,
                space: [0u8; 120],
            },
        );

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(after_close_market_token_0_balance, after_open_market_token_0_balance);
        assert_eq!(after_close_market_token_1_balance, after_open_market_token_1_balance - 10051663);
        assert_eq!(after_close_fee_collector_quote_balance, after_open_fee_collector_quote_balance + 10051663);
        assert_eq!(after_close_market_raydium_lp_token_balance, after_open_market_raydium_lp_token_balance);
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions,
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 40206653,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance,
        );
    }

    #[tokio::test]
    async fn test_close_position_external_rollover_different_duration() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
        }).await;
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

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 10_000000000,
            is_short: true,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 20_000000000,
            worst_price_num: 900,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &user,
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
            quote_token_balance: 39697658947+20_000000000,
            lp_tokens_removed: 626505013,
            loan_position_token_amt: 20022037039,
            loan_collateral_token_amt: 19605823,
            is_short: args.is_short,
            open_block: 100,
            close_block: 100+args.duration_blocks,
            open_base_token_amt: 1_000000000,
            open_quote_token_amt: 1000_000000000,
            after_open_base_token_amt: 989794029,
            after_open_quote_token_amt: 970962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1200, 1).await;

        helpers::edit_position_rollover(
            &mut t,
            &user,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::EditPositionRolloverArgs{
                id: args.id,
                max_fee_quote_token_amt: None,
                rollover_duration: Some(2000000),
                new_rollover_reserve_quote_token_amt: None,
            }
        ).await.unwrap();

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        tutils::token::get_lamports(&mut t, &closer.pubkey(), 100_000000000).await;
        helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &user.pubkey(), args.id).unwrap();
        assert_eq!(
            t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
            limitless::state::position::PositionAccount{
                id: args.id,
                market_account: market_account.clone(),
                position_size: 39647400631,
                user_quote_token_collateral_amt: args.collateral_quote_token_amt,
                collateral_amt: 9900990,
                raydium_fee_reserve_collateral_token_amt: 299030,
                rounding_fee_reserve_collateral_token_amt: 0,
                raydium_fees_reserved_amt_quote_token: 613460517,
                blackwing_fee_reserve_quote_token_amt: 121802035,
                base_token_balance: 10200031,
                quote_token_balance: 39697658947+20_000000000-50258316,
                lp_tokens_removed: 626505013,
                loan_position_token_amt: 20022037039,
                loan_collateral_token_amt: 19605823,
                is_short: args.is_short,
                open_block: 100,
                close_block: 1000200+2000000,
                open_base_token_amt: 1_000000000,
                open_quote_token_amt: 1000_000000000,
                after_open_base_token_amt: 989794029,
                after_open_quote_token_amt: 970962875848,
                rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
                rollover_duration_blocks: 2000000,
                rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt-121802035,
                space: [0u8; 120],
            },
        );

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(after_close_market_token_0_balance, after_open_market_token_0_balance);
        assert_eq!(after_close_market_token_1_balance, after_open_market_token_1_balance - 10051663);
        assert_eq!(after_close_fee_collector_quote_balance, after_open_fee_collector_quote_balance + 10051663);
        assert_eq!(after_close_market_raydium_lp_token_balance, after_open_market_raydium_lp_token_balance);
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions,
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 40206653,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance,
        );
    }

    #[tokio::test]
    async fn test_close_position_rollover_not_enough_fees() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
        }).await;
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

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 49306450,
            worst_price_num: 0,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &user,
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
            quote_token_balance: 10249761190+49306450,
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &user.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                - 49306450 // rollover reserve
                + 39792681 // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                + 1_225848332 // pnl
                + 49306450 // rollover reserve amount returned
        );
    }

    #[tokio::test]
    async fn test_close_position_rollover_max_rollover_fee_exceeded() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
        }).await;
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

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 53642093,
            rollover_reserve_quote_token_amt: 49306450000,
            worst_price_num: 0,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &user,
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
            quote_token_balance: 10249761190+49306450000,
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &user.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                - 49306450000 // rollover reserve
                + 39792681 // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_close_user_token_1_balance,
            after_open_user_token_1_balance
                + args.collateral_quote_token_amt
                + 1_225848332 // pnl
                + 49306450000 // rollover reserve amount returned
        );
    }

    #[tokio::test]
    async fn test_close_position_external_rollover_before_duration_expiration() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
        }).await;
        let token_0 = result.token_0;
        let token_1 = result.token_1;
        let market_account = derive_market_account_pda(&token_0.pubkey(), &token_1.pubkey()).unwrap();

        // So we have a consistent state in the test.
        let lp_supply = tutils::raydium::get_pool_state(&mut t, &token_0.pubkey(), &token_1.pubkey())
            .await.lp_supply;
        assert_eq!(lp_supply, 31622776601);

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 49306450000,
            worst_price_num: 0,
            worst_price_den: 1,
        };
        let pos = helpers::open_position(
            &mut t,
            &user,
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
            quote_token_balance: 10249761190+49306450000,
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        t.set_slot(200).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        let res = helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::InvalidSigner);

        let position_account_key = derive_position_account_pda(&market_account, &user.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_close_position_slippage_dont_check_because_duration_expired() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
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
            collateral_quote_token_amt: 10_000000000,
            is_short: true,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 900,
            worst_price_den: 1,
        };
        helpers::open_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            args.clone(),
        ).await.unwrap();

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1200, 1).await;

        t.set_slot(1000100).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 2000,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_close_position_slippage_long() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        t.set_slot(200).await;

        // Close position.
        let res = helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 2000,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::SlippageExceeded);

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_close_position_slippage_short() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: false,
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
            collateral_quote_token_amt: 10_000000000,
            is_short: true,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 0,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 0,
            worst_price_num: 900,
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
            after_open_quote_token_amt: 970962875848,
            rollover_max_fee_quote_token_amt: args.max_blackwing_fee_quote_token_amt,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0u8; 120],
        });

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 900, 1).await;

        t.set_slot(200).await;

        // Close position.
        let res = helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 100,
                worst_price_den: 1,
            },
        ).await;
        assert_err(res, LimitlessError::SlippageExceeded);

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_close_position_unwrap_sol() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: true,
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let position_account_key = derive_position_account_pda(
            &market_account,
            &creator.pubkey(),
            pos.id,
        ).unwrap();
        let after_open_user_position_lamports = t.banks_client().get_balance(position_account_key).await.unwrap();
        let after_open_user_sol_balance = t.banks_client().get_balance(creator.pubkey()).await.unwrap();
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        t.set_slot(1000200).await;

        // Close position.
        helpers::close_position(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &creator.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                + 39792681, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_close_user_sol_balance = t.banks_client().get_balance(creator.pubkey()).await.unwrap();
        let after_close_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(after_close_user_token_1_balance, after_open_user_token_1_balance);
        assert_eq!(
            after_close_user_sol_balance,
            after_open_user_sol_balance
                + after_open_user_position_lamports
                + args.collateral_quote_token_amt
                + 1_225848332 // pnl (tx paid by payer in test)
        );
        assert_eq!(after_close_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_close_position_unwrap_sol_external() {
        let fee_collector_override = Pubkey::new_unique();
        let (mut t, result) = helpers::setup(helpers::SetupParams{
            pool_token_0_amt: 1_000000000,
            pool_token_1_amt: 1000_000000000,
            raydium_trade_fee: 10_000,
            raydium_fund_fee: 10_000,
            raydium_protocol_fee: 20_000,
            quote_token: QuoteToken::Token1,
            fee_collector_override: Some(fee_collector_override),
            quote_token_wsol: true,
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

        let user = Keypair::new();
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_0.pubkey()).await;
        tutils::token::create_token_account(&mut t, &user.pubkey(), &token_1.pubkey()).await;
        tutils::token::get_lamports(&mut t, &user.pubkey(), 1_000000000).await;

        // Give the user money.
        token_1.mint(&mut t, &user.pubkey(), 100_000000000).await;

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
            &user,
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

        helpers::mock_pool_price_approx(&mut t, &token_0, &token_1, QuoteToken::Token1, 1100, 1).await;

        // After open market and user state.
        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;
        let position_account_key = derive_position_account_pda(
            &market_account,
            &creator.pubkey(),
            pos.id,
        ).unwrap();
        let after_open_user_position_lamports = t.banks_client().get_balance(position_account_key).await.unwrap();
        let after_open_user_sol_balance = t.banks_client().get_balance(user.pubkey()).await.unwrap();

        t.set_slot(1000200).await;

        // Close position.
        let closer = load_limitless_closer_keypair();
        tutils::token::get_lamports(&mut t, &closer.pubkey(), 100_000000000).await;
        let after_open_closer_sol_balance = t.banks_client().get_balance(closer.pubkey()).await.unwrap();
        helpers::close_position_as_external(
            &mut t,
            &user.pubkey(),
            &closer,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::ClosePositionArgs{
                id: args.id,
                worst_price_num: 0,
                worst_price_den: 1,
            },
        ).await.unwrap();

        let position_account_key = derive_position_account_pda(&market_account, &user.pubkey(), args.id).unwrap();
        assert!(t.banks_client().get_account(position_account_key).await.unwrap().is_none());

        // Assert market state.
        let after_close_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_close_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_close_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_close_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_close_fee_collector_quote_balance = get_fee_collector_quote_token_balance(&mut t, &token_0.pubkey(), &token_1.pubkey()).await;
        assert_eq!(
            after_close_market_token_0_balance,
            after_open_market_token_0_balance - 38831218 // position token 0 balance,
        );
        assert_eq!(
            after_close_market_token_1_balance,
            after_open_market_token_1_balance
                - 10249761190 // position token 1 balance
                + 39792681, // blackwing fees
        );
        assert_eq!(
            after_close_fee_collector_quote_balance,
            after_open_fee_collector_quote_balance + 9948170
        );
        assert_eq!(
            after_close_market_raydium_lp_token_balance,
            after_open_market_raydium_lp_token_balance + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_tokens_removed_for_positions,
            after_open_market_account.lp_tokens_removed_for_positions - 620054443 // position lp tokens
        );
        assert_eq!(
            after_close_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
                - 620054443 // position lp tokens
                + 602993749 // expected lp tokens minted on repayment
        );
        assert_eq!(
            after_close_market_account.lp_token_fee_balance_pool.total_balance(),
            after_open_market_account.lp_token_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_0_fee_balance_pool.total_balance(),
            after_open_market_account.token_0_fee_balance_pool.total_balance(),
        );
        assert_eq!(
            after_close_market_account.token_1_fee_balance_pool.total_balance(),
            after_open_market_account.token_1_fee_balance_pool.total_balance() + 39792681,
        );

        // Assert user state.
        let after_close_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &user.pubkey()).await;
        let after_close_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &user.pubkey()).await;
        let after_close_user_sol_balance = t.banks_client().get_balance(user.pubkey()).await.unwrap();
        let after_close_closer_sol_balance = t.banks_client().get_balance(closer.pubkey()).await.unwrap();
        assert_eq!(after_close_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(after_close_user_token_1_balance, after_open_user_token_1_balance);
        assert_eq!(after_close_closer_sol_balance, after_open_closer_sol_balance);
        assert_eq!(
            after_close_user_sol_balance,
            after_open_user_sol_balance
                + after_open_user_position_lamports
                + args.collateral_quote_token_amt
                + 1_229084732 // pnl (tx fees paid by payer in test)
        );
    }
}
