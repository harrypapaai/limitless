mod helpers;

use solana_program::pubkey::Pubkey;
use solana_program_test::{processor, tokio};
use limitless::client::accounts::derive_limitless_config_pda;
use limitless::state::config::TradingMode;

#[tokio::test]
async fn test_admin_init() {
    let t = &mut tutils::new_t()
        .with_raydium(None)
        .with_program(
            "limitless",
            limitless::ID,
            processor!(limitless::entrypoint::process_instruction),
        )
        .build().await;

    let fee_collector = Pubkey::new_unique();

    helpers::admin_init_limitless(t, &fee_collector).await;

    // Validate the config account was created successfully.
    let config_account_key = derive_limitless_config_pda().unwrap();
    let config_account = t.get_account::<limitless::state::config::ConfigAccount>(&config_account_key).await;
    assert_eq!(config_account, limitless::state::config::ConfigAccount {
        trading_mode: TradingMode::Enabled,
        fee_collector,
        space: [0; 128],
    })
}
