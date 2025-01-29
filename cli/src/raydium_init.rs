use clap::{Arg, ArgMatches, Command};
use solana_clap_v3_utils::keypair::signer_from_path;
use solana_sdk::signature::Signer;
use solana_sdk::transaction::Transaction;
use crate::utils::{get_shared_args, get_rpc_client, get_rent};

pub(crate) async fn init(matches: &ArgMatches) {
    let rpc_client = get_rpc_client(matches);
    let deployer = matches.value_of("deployer").unwrap();
    let deployer_signer = signer_from_path(
        matches, deployer, "deployer", &mut None).unwrap();

    let rent = get_rent(&rpc_client).await;

    let (raydium_init_instructions, raydium_init_signers) =
        tutils::raydium::instructions::init(&deployer_signer.pubkey(), rent, None);

    let blockhash = rpc_client.get_latest_blockhash().await.unwrap();

    let mut transaction = Transaction::new_with_payer(
        raydium_init_instructions.as_slice(),
        Some(&deployer_signer.pubkey()),
    );

    let mut all_signers: Vec<Box<dyn Signer>> = raydium_init_signers
        .into_iter()
        .map(|keypair| Box::new(keypair) as Box<dyn Signer>)
        .collect();
    all_signers.push(deployer_signer);

    transaction.sign(
        &all_signers,
        blockhash,
    );
    rpc_client.send_and_confirm_transaction(&transaction).await.unwrap();
}

pub(crate) fn cmd() -> Command<'static> {
    Command::new("raydium-init").
        args(&get_shared_args()).
        arg(
            Arg::new("deployer").
                long("deployer").
                short('d').
                help("path to deployer signer").
                takes_value(true).
                required(true)
        )
}