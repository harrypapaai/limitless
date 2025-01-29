#![allow(warnings)]

mod raydium_init;
mod utils;
mod open_limitless_position;
mod mint_wsol;
mod deposit_limitless_liquidity;
mod create_token_account;
mod get_token_balance;
mod close_limitless_position;
mod limitless_init_admin;
mod update_market_config;
mod init_limitless_market;
mod squads;
mod withdraw_limitless_liquidity;
mod raydium_init_config;
mod create_metadata;
mod raydium_init_pool;
mod get_limitless_accounts;
mod get_nft_holders;
mod helius_utils;
mod get_token_holders;
mod batch_transfer_tokens;
mod raydium_deposit;
mod raydium_collect_protocol_fee;
mod parse_limitless_ix;
mod transfer_spl;

use clap::{Command};
use bincode;
use solana_sdk::signature::{Signer};
use solana_sdk::sysvar::{SysvarId};
use crate::create_token_account::create_token_account;

#[tokio::main]
async fn main() {
    let cmd = Command::new("blackwing cli")
        .subcommand(
            raydium_init::cmd()
        )
        .subcommand(
            open_limitless_position::cmd()
        )
        .subcommand(
            close_limitless_position::cmd()
        )
        .subcommand(
            mint_wsol::cmd()
        ).subcommand(
            transfer_spl::cmd()
        ).subcommand(
            create_token_account::cmd()
        )
        .subcommand(
            get_token_balance::cmd()
        )
        .subcommand(
            limitless_init_admin::cmd()
        )
        .subcommand(
            init_limitless_market::cmd()
        ).subcommand(
            deposit_limitless_liquidity::cmd()
        )
        .subcommand(
            update_market_config::cmd()
        )
        .subcommand(
            withdraw_limitless_liquidity::cmd()
        ).subcommand(
            raydium_init_config::cmd()
        ).subcommand(
            create_metadata::cmd()
        ).subcommand(
           raydium_init_pool::cmd()
        ).subcommand(
           get_limitless_accounts::cmd()
        ).subcommand(
            get_nft_holders::cmd()
        ).subcommand(
            get_token_holders::cmd()
        ).subcommand(
            batch_transfer_tokens::cmd()
        ).subcommand(
            raydium_deposit::cmd()
        ).subcommand(
            raydium_collect_protocol_fee::cmd()
        ).subcommand(
            parse_limitless_ix::cmd()
        ).get_matches();

    if let Some(matches) = cmd.subcommand_matches("raydium-init") {
        raydium_init::init(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("open-limitless-position") {
        open_limitless_position::open_limitless_position(matches).await;
    }  else if let Some(matches) = cmd.subcommand_matches("close-limitless-position") {
        close_limitless_position::close_limitless_position(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("mint-wsol") {
        mint_wsol::mint_wsol(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("transfer-spl") {
        transfer_spl::transfer_spl(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("create-token-account") {
        create_token_account(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("get-token-balance") {
        get_token_balance::get_token_balance(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("init-limitless-admin") {
        limitless_init_admin::init(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("update-market-config") {
        update_market_config::update_market_config(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("init-limitless-market") {
        init_limitless_market::init(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("deposit-limitless-liquidity") {
        deposit_limitless_liquidity::deposit_limitless_liquidity(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("withdraw-limitless-liquidity") {
        withdraw_limitless_liquidity::withdraw_limitless_liquidity(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("raydium-init-config") {
        raydium_init_config::init(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("create-metadata") {
        create_metadata::create_metadata(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("raydium-init-pool") {
        raydium_init_pool::init_pool(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("get-limitless-accounts") {
        get_limitless_accounts::get_limitless_accounts(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("get-nft-holders") {
        get_nft_holders::get_nft_owners(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("get-token-holders") {
        get_token_holders::get_token_holders(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("batch-transfer-tokens") {
        batch_transfer_tokens::batch_transfer_tokens(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("raydium-deposit") {
        raydium_deposit::raydium_deposit(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("raydium-collect-protocol-fee") {
        raydium_collect_protocol_fee::raydium_collect_protocol_fee(matches).await;
    } else if let Some(matches) = cmd.subcommand_matches("parse-limitless-ix") {
        parse_limitless_ix::parse_limitless_ix(matches).await;
    }
}
