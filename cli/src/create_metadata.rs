use clap::{Arg, ArgMatches, Command};
use mpl_token_metadata::instructions::{CreateMetadataAccountV3, CreateMetadataAccountV3InstructionArgs};
use mpl_token_metadata::types::DataV2;
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_program::pubkey::Pubkey;
use solana_program::sysvar;
use solana_sdk::transaction::Transaction;
use crate::utils::{self, get_rpc_client, get_shared_args};

pub(crate) async fn create_metadata(args: &ArgMatches) {
    let rpc_client = get_rpc_client(args);
    let payer = args.value_of("payer").unwrap();
    let payer_signer = signer_from_path(
        args, payer, "payer", &mut None).unwrap();

    let token_mint: Pubkey = args.value_of("token").unwrap().parse().unwrap();

    let mint_authority = args.value_of("mint-authority").unwrap();
    let mint_authority_signer = signer_from_path(
        args, mint_authority, "mint-authority", &mut None).unwrap();

    let (metadata_pda, _bump_seed) = Pubkey::find_program_address(
        &[
            b"metadata",
            mpl_token_metadata::ID.as_ref(),
            token_mint.as_ref(),
        ],
        &mpl_token_metadata::ID,
    );
    let data = DataV2 {
        name: "".to_string(),
        symbol: "".to_string(),
        uri: "".to_string(),
        seller_fee_basis_points: 0,
        creators: None,
        collection: None,
        uses: None,
    };
    let create_metadata_account = CreateMetadataAccountV3{
        metadata: metadata_pda,
        mint: token_mint,
        mint_authority: mint_authority_signer.pubkey(),
        payer: payer_signer.pubkey(),
        update_authority: (payer_signer.pubkey(), false),
        system_program: solana_program::system_program::ID,
        rent: Some(sysvar::rent::ID),
    };
    let instruction = create_metadata_account.instruction(
        CreateMetadataAccountV3InstructionArgs{
            data,
            is_mutable: true,
            collection_details: None,
        }
    );
    let mut transaction = Transaction::new_with_payer(
        &[instruction],
        Some(&payer_signer.pubkey()),
    );
    transaction.sign(
        &[payer_signer.as_ref(), mint_authority_signer.as_ref()],
        rpc_client.get_latest_blockhash().await.unwrap(),
    );
    rpc_client.send_and_confirm_transaction(&transaction).await.unwrap();
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("create-metadata").
        args(&get_shared_args()).
        arg(
            Arg::new("payer").
                long("payer").
                short('p').
                help("path to the payer who is opening the position").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("token").
                long("token").
                short('t').
                help("mint of token to create metadata for").
                takes_value(true).
                required(true)
        ).
        arg(
            Arg::new("mint-authority").
                long("mint-authority").
                short('m').
                help("mint authority of token to create metadata for").
                takes_value(true).
                required(true)
        )
}
