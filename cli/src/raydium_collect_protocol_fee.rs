use std::str::FromStr;
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::token_2022::spl_token_2022;
use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use crate::squads::create_transaction;
use crate::utils::{get_shared_args, get_rpc_client};

const SENTINAL: &str = "sentinal";

pub(crate) async fn raydium_collect_protocol_fee(matches: &ArgMatches) {
    let rpc_client = get_rpc_client(matches);

    // who will be paying for this transaction
    let payer = matches.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        matches, payer, "payer", &mut None).unwrap();

    let token0_mint = Pubkey::from_str(matches.value_of("token0").unwrap()).unwrap();
    let token1_mint = Pubkey::from_str(matches.value_of("token1").unwrap()).unwrap();
    let fee_collector = Pubkey::from_str(matches.value_of("fee-collector").unwrap()).unwrap();
    let fee_collector_token_0_ata = get_associated_token_address(&fee_collector, &token0_mint);
    let fee_collector_token_1_ata = get_associated_token_address(&fee_collector, &token1_mint);

    let authority = utils::raydium::pda::authority_pda().unwrap();
    let pool_state = utils::raydium::pda::pool_state_pda(&token0_mint, &token1_mint).unwrap();
    let amm_config = utils::raydium::pda::amm_config_pda().unwrap();
    let (token0_vault, token1_vault) = utils::raydium::pda::token_vault_pdas(&token0_mint, &token1_mint).unwrap();

    let collect_fee_accounts = utils::raydium::raydium_cp_swap::client::accounts::CollectProtocolFee {
        owner: fee_collector,
        authority,
        pool_state,
        amm_config,
        token0_vault,
        token1_vault,
        vault0_mint: token0_mint,
        vault1_mint: token1_mint,
        recipient_token0_account: fee_collector_token_0_ata,
        recipient_token1_account: fee_collector_token_1_ata,
        token_program: spl_token::ID,
        token_program2022: spl_token_2022::ID,
    };

    let collect_fee_args = utils::raydium::raydium_cp_swap::client::args::CollectProtocolFee {
        amount0_requested: 0,
        amount1_requested: 100,
    };

    let collect_fee_args_data = collect_fee_args.data();

    let collect_fee_ix = Instruction::new_with_bytes(
        utils::raydium::ID,
        collect_fee_args_data.as_slice(),
        collect_fee_accounts.to_account_metas(None),
    );

    let ms_key_raw = matches.value_of("multisig").unwrap();
    if ms_key_raw == SENTINAL {
        let mut tx = Transaction::new_with_payer(
            &[collect_fee_ix],
            Some(&payer_signer.pubkey()),
        );
        tx.sign(
            &[payer_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        let signature = rpc_client.send_and_confirm_transaction(&tx).await.expect("could not collect fees");
        println!("collected fees, signature: {}", signature);
    } else {
        let ms_key: Pubkey = Pubkey::from_str(ms_key_raw).unwrap();
        let mut ms_create_transaction_tx = create_transaction(
            ms_key, payer_signer.pubkey(), vec![collect_fee_ix], &rpc_client).await;

        ms_create_transaction_tx.sign(
            &[payer_signer.as_ref()],
            rpc_client.get_latest_blockhash().await.unwrap(),
        );
        let ms_create_transaction_tx_sig = rpc_client.send_and_confirm_transaction(&ms_create_transaction_tx).await.unwrap();
        println!("Created squads transaction {:?}", ms_create_transaction_tx_sig);
    }
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("raydium-collect-protocol-fee").
        args(&get_shared_args()).
        arg(
            Arg::new("payer").
                long("payer").
                short('p').
                help("payer that will pay for the transaction").
                takes_value(true).
                required(true)
        ).arg(
            Arg::new("multisig").
                long("multisig").
                short('m').
                help("multisig address for admin authority").
                takes_value(true).
                default_value(SENTINAL)
        ).
        arg(
            Arg::new("token0").
                long("token0").
                help("token0 mint").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("token1").
                long("token1").
                help("token1 mint").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("fee-collector").
                long("fee-collector").
                help("fee collector address").
                takes_value(true).
                required(true)
        )
}