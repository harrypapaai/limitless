use std::io::Cursor;
use tutils::{token, raydium, load_creator_admin_keypair};

use solana_sdk::signature::{Keypair};
use {
    solana_program_test::*,
    solana_sdk::{signature::Signer},
};

// Uncomment to generate a new keypair.
#[test]
fn test() {
    use solana_sdk::signature::{write_keypair_file};
    let k = Keypair::new();
    println!("{:?}", k.pubkey());
    write_keypair_file(&k, "test.keypair").unwrap();
}

#[tokio::test]
async fn test_init_raydium_market_and_swap() {
    let mut t = tutils::new_t().with_raydium(None).build().await;
    let mut token_0 = token::create_mock_token(&mut t, 9).await;
    let mut token_1 = token::create_mock_token(&mut t, 6).await;
    if token_0.pubkey() >= token_1.pubkey() {
        let temp = token_0.insecure_clone();
        token_0 = token_1;
        token_1 = temp;
    }

    let creator = Keypair::new();

    token::mint_mock_token(&mut t, &token_0, &creator.pubkey(), 1_000000000).await;
    token::mint_mock_token(&mut t, &token_1, &creator.pubkey(), 100000_000000).await;

    let token_0_balance = token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
    let token_1_balance = token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
    assert_eq!(token_0_balance, 1_000000000);
    assert_eq!(token_1_balance, 100000_000000);

    raydium::exec_init_market(
        &mut t,
        &creator,
        &token_0.pubkey(),
        &token_1.pubkey(),
        1_000000000,
        100000_000000,
    ).await;

    let token_0_balance = token::get_token_balance(&mut t, &token_0.pubkey(), &creator.pubkey()).await;
    let token_1_balance = token::get_token_balance(&mut t, &token_1.pubkey(), &creator.pubkey()).await;
    assert_eq!(token_0_balance, 0);
    assert_eq!(token_1_balance, 0);

    let pool_price = raydium::get_price_x32(
        &mut t,
        &token_0.pubkey(),
        &token_1.pubkey(),
    ).await;
    assert_eq!(pool_price, 100000_000000u128 * utils::raydium::Q32 / 1_000000000u128);
}
