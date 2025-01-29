mod helpers;

use solana_program_test::tokio;
use solana_sdk::signature::Signer;
use limitless::client::accounts::{derive_market_account_pda, derive_market_token_account_pda};
use limitless::state::market::QuoteToken;

#[cfg(test)]
mod test {
   use super::*;

    #[tokio::test]
    async fn test_edit_position_rollover_no_change() {
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

        // Edit settings.
        helpers::edit_position_rollover(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::EditPositionRolloverArgs{
                id: args.id,
                max_fee_quote_token_amt: None,
                rollover_duration: None,
                new_rollover_reserve_quote_token_amt: None,
            },
        ).await.unwrap();

        // Assert position state.
        let position_account_key = limitless::client::accounts::derive_position_account_pda(
            &market_account,
            &creator.pubkey(),
            args.id,
        ).unwrap();
        assert_eq!(
            t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
            limitless::state::position::PositionAccount{
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
            },
        );
    }

    #[tokio::test]
    async fn test_edit_position_rollover_no_reserve_change() {
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

        // Edit settings.
        helpers::edit_position_rollover(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::EditPositionRolloverArgs{
                id: args.id,
                max_fee_quote_token_amt: Some(10_000000),
                rollover_duration: Some(20_000000),
                new_rollover_reserve_quote_token_amt: None,
            },
        ).await.unwrap();

        // Assert position state.
        let position_account_key = limitless::client::accounts::derive_position_account_pda(
            &market_account,
            &creator.pubkey(),
            args.id,
        ).unwrap();
        assert_eq!(
        t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
        limitless::state::position::PositionAccount{
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
            rollover_max_fee_quote_token_amt: 10_000000,
            rollover_duration_blocks: 20_000000,
            rollover_reserve_quote_token_amt: args.rollover_reserve_quote_token_amt,
            space: [0; 120],
        },
    );
    }

    #[tokio::test]
    async fn test_edit_position_rollover_increase_reserve_balance() {
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

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 10_000000000,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 49306451,
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
            quote_token_balance: 10249761190+args.rollover_reserve_quote_token_amt,
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

        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        // Edit settings.
        helpers::edit_position_rollover(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::EditPositionRolloverArgs{
                id: args.id,
                max_fee_quote_token_amt: Some(10_000000),
                rollover_duration: None,
                new_rollover_reserve_quote_token_amt: Some(10_000000000),
            },
        ).await.unwrap();

        // Assert position state.
        let position_account_key = limitless::client::accounts::derive_position_account_pda(
            &market_account,
            &creator.pubkey(),
            args.id,
        ).unwrap();
        assert_eq!(
        t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
        limitless::state::position::PositionAccount{
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
            quote_token_balance: 10249761190+10_000000000,
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
            rollover_max_fee_quote_token_amt: 10_000000,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: 10_000000000,
            space: [0; 120],
        },
    );

        // Assert market state.
        let after_edit_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_edit_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_edit_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_edit_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(after_edit_market_token_0_balance, after_open_market_token_0_balance);
        assert_eq!(
        after_edit_market_token_1_balance,
        after_open_market_token_1_balance
            + 10_000000000 - args.rollover_reserve_quote_token_amt,
    );
        assert_eq!(after_edit_market_raydium_lp_token_balance, after_open_market_raydium_lp_token_balance);
        assert_eq!(after_edit_market_account.lp_tokens_removed_for_positions, after_open_market_account.lp_tokens_removed_for_positions);
        assert_eq!(
            after_edit_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
        );
        assert_eq!(
        after_edit_market_account.lp_token_fee_balance_pool.total_balance(),
        after_open_market_account.lp_token_fee_balance_pool.total_balance(),
    );
        assert_eq!(
        after_edit_market_account.token_0_fee_balance_pool.total_balance(),
        after_open_market_account.token_0_fee_balance_pool.total_balance(),
    );
        assert_eq!(
        after_edit_market_account.token_1_fee_balance_pool.total_balance(),
        after_open_market_account.token_1_fee_balance_pool.total_balance(),
    );

        // Assert user state.
        let after_edit_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_edit_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_edit_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_edit_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_edit_user_token_1_balance,
            after_open_user_token_1_balance
                - (10_000000000 - args.rollover_reserve_quote_token_amt)
        );
        assert_eq!(after_edit_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_edit_position_rollover_decrease_reserve_balance() {
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

        t.set_slot(100).await;

        // Open position.
        let args = limitless::instructions::OpenPositionArgs{
            id: uuid::Uuid::new_v4(),
            collateral_quote_token_amt: 9_900990099,
            is_short: false,
            duration_blocks: 1000000,
            max_raydium_fee_quote_token_amt: 10_000000000,
            max_blackwing_fee_quote_token_amt: 0,
            rollover_reserve_quote_token_amt: 49306451,
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
            quote_token_balance: 10249761190+args.rollover_reserve_quote_token_amt,
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

        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        // Edit settings.
        helpers::edit_position_rollover(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::EditPositionRolloverArgs{
                id: args.id,
                max_fee_quote_token_amt: None,
                rollover_duration: Some(10_000000),
                new_rollover_reserve_quote_token_amt: Some(29306451),
            },
        ).await.unwrap();

        // Assert position state.
        let position_account_key = limitless::client::accounts::derive_position_account_pda(
            &market_account,
            &creator.pubkey(),
            args.id,
        ).unwrap();
        assert_eq!(
            t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
            limitless::state::position::PositionAccount{
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
                quote_token_balance: 10249761190+29306451,
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
                rollover_duration_blocks: 10_000000,
                rollover_reserve_quote_token_amt: 29306451,
                space: [0; 120],
            },
        );

        // Assert market state.
        let after_edit_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_edit_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_edit_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_edit_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(after_edit_market_token_0_balance, after_open_market_token_0_balance);
        assert_eq!(
        after_edit_market_token_1_balance,
        after_open_market_token_1_balance
            + 29306451 - args.rollover_reserve_quote_token_amt,
    );
        assert_eq!(after_edit_market_raydium_lp_token_balance, after_open_market_raydium_lp_token_balance);
        assert_eq!(after_edit_market_account.lp_tokens_removed_for_positions, after_open_market_account.lp_tokens_removed_for_positions);
        assert_eq!(
            after_edit_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
        );
        assert_eq!(
        after_edit_market_account.lp_token_fee_balance_pool.total_balance(),
        after_open_market_account.lp_token_fee_balance_pool.total_balance(),
    );
        assert_eq!(
        after_edit_market_account.token_0_fee_balance_pool.total_balance(),
        after_open_market_account.token_0_fee_balance_pool.total_balance(),
    );
        assert_eq!(
        after_edit_market_account.token_1_fee_balance_pool.total_balance(),
        after_open_market_account.token_1_fee_balance_pool.total_balance(),
    );

        // Assert user state.
        let after_edit_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_edit_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_edit_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_edit_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_edit_user_token_1_balance,
            after_open_user_token_1_balance
                - 29306451 + args.rollover_reserve_quote_token_amt
        );
        assert_eq!(after_edit_user_lp_token_balance, after_open_user_lp_token_balance);
    }

    #[tokio::test]
    async fn test_edit_position_rollover_decrease_entire_reserve_balance() {
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
            quote_token_balance: 10249761190+args.rollover_reserve_quote_token_amt,
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

        let after_open_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_open_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_open_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_open_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        let after_open_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_open_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_open_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;

        // Edit settings.
        helpers::edit_position_rollover(
            &mut t,
            &creator,
            &token_0.pubkey(),
            &token_1.pubkey(),
            limitless::instructions::EditPositionRolloverArgs{
                id: args.id,
                max_fee_quote_token_amt: Some(10_000000),
                rollover_duration: None,
                new_rollover_reserve_quote_token_amt: Some(0),
            },
        ).await.unwrap();

        // Assert position state.
        let position_account_key = limitless::client::accounts::derive_position_account_pda(
            &market_account,
            &creator.pubkey(),
            args.id,
        ).unwrap();
        assert_eq!(
        t.get_account::<limitless::state::position::PositionAccount>(&position_account_key).await,
        limitless::state::position::PositionAccount{
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
            rollover_max_fee_quote_token_amt: 10_000000,
            rollover_duration_blocks: args.duration_blocks,
            rollover_reserve_quote_token_amt: 0,
            space: [0; 120],
        },
    );

        // Assert market state.
        let after_edit_market_token_0_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_0_ata).await;
        let after_edit_market_token_1_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_token_1_ata).await;
        let after_edit_market_raydium_lp_token_balance = tutils::token::get_token_balance_from_token_account(&mut t, &market_raydium_lp_token_ata).await;
        let after_edit_market_account = t.get_account::<limitless::state::market::MarketAccount>(&market_account).await;
        assert_eq!(after_edit_market_token_0_balance, after_open_market_token_0_balance);
        assert_eq!(
        after_edit_market_token_1_balance,
        after_open_market_token_1_balance
            - args.rollover_reserve_quote_token_amt,
    );
        assert_eq!(after_edit_market_raydium_lp_token_balance, after_open_market_raydium_lp_token_balance);
        assert_eq!(after_edit_market_account.lp_tokens_removed_for_positions, after_open_market_account.lp_tokens_removed_for_positions);
        assert_eq!(
            after_edit_market_account.lp_tokens_supplied_pool.total_balance(),
            after_open_market_account.lp_tokens_supplied_pool.total_balance()
        );
        assert_eq!(
        after_edit_market_account.lp_token_fee_balance_pool.total_balance(),
        after_open_market_account.lp_token_fee_balance_pool.total_balance(),
    );
        assert_eq!(
        after_edit_market_account.token_0_fee_balance_pool.total_balance(),
        after_open_market_account.token_0_fee_balance_pool.total_balance(),
    );
        assert_eq!(
        after_edit_market_account.token_1_fee_balance_pool.total_balance(),
        after_open_market_account.token_1_fee_balance_pool.total_balance(),
    );

        // Assert user state.
        let after_edit_user_token_0_balance = tutils::token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
        let after_edit_user_token_1_balance = tutils::token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
        let after_edit_user_lp_token_balance = tutils::token::get_token_balance(&mut t, &raydium_lp_token, &creator.pubkey()).await;
        assert_eq!(after_edit_user_token_0_balance, after_open_user_token_0_balance);
        assert_eq!(
            after_edit_user_token_1_balance,
            after_open_user_token_1_balance
                + args.rollover_reserve_quote_token_amt
        );
        assert_eq!(after_edit_user_lp_token_balance, after_open_user_lp_token_balance);
    }
}
