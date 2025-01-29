use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::instruction::{AccountMeta, Instruction};
use solana_program::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;
use utils::squads::squads_mpl::accounts::ms;

pub async fn get_ms_account(ms_key: &Pubkey, rpc_client: &RpcClient) -> ms {
    let ms_account = rpc_client.get_account(&ms_key).await.unwrap();
    ms::try_deserialize_unchecked(&mut ms_account.data.as_slice()).unwrap()
}

pub async fn create_transaction(
    multisig: Pubkey,
    creator: Pubkey,
    incoming_instructions: Vec<Instruction>,
    rpc_client: &RpcClient,
) -> Transaction {
    let account = get_ms_account(&multisig, &rpc_client).await;

    let (transaction_pda, _) = Pubkey::find_program_address(
        &[
            b"squad",
            multisig.as_ref(),
            &account.transaction_index.checked_add(1).unwrap().to_le_bytes(),
            b"transaction"],
        &utils::squads::squads_mpl::ID
    );

    let mut instructions = Vec::new();
    let create_transaction_ix = create_transaction_ix(multisig, transaction_pda,
                                                      creator, account.authority_index as u32);
    instructions.push(create_transaction_ix);

    let mut instruction_index = 1;
    for incoming_instruction in incoming_instructions.iter() {
        let add_instruction_ix = add_instruction_ix(multisig, transaction_pda,
                                                    creator, instruction_index, incoming_instruction);
        instructions.push(add_instruction_ix);
        instruction_index += 1;
    }

    let activate_transaction_ix = activate_transaction_ix(multisig, transaction_pda, creator);
    instructions.push(activate_transaction_ix);

    Transaction::new_with_payer(
        instructions.as_slice(),
        Some(&creator),
    )
}

pub fn create_transaction_ix(multisig: Pubkey,
                             transaction_pda: Pubkey,
                             creator: Pubkey,
                             authority_index: u32) -> Instruction {
    let ms_create_transaction_accounts = utils::squads::squads_mpl::client::accounts::CreateTransaction{
        multisig,
        transaction: transaction_pda,
        creator,
        system_program: solana_program::system_program::ID,
    };
    let ms_create_transaction_args = utils::squads::squads_mpl::client::args::CreateTransaction {
        authority_index,
    };
    let ms_create_transaction_data = ms_create_transaction_args.data();
    Instruction::new_with_bytes(
        utils::squads::squads_mpl::ID,
        ms_create_transaction_data.as_slice(),
        ms_create_transaction_accounts.to_account_metas(None),
    )
}

pub fn add_instruction_ix(multisig: Pubkey,
                          transaction_pda: Pubkey,
                          creator: Pubkey,
                          instruction_index: u8,
                          incoming_instruction: &Instruction) -> Instruction {
    let (instruction_pda, _) = Pubkey::find_program_address(
        &[
            b"squad",
            transaction_pda.as_ref(),
            &instruction_index.to_le_bytes(),
            b"instruction"
        ],
        &utils::squads::squads_mpl::ID
    );

    let accounts = utils::squads::squads_mpl::client::accounts::AddInstruction{
        multisig,
        transaction: transaction_pda,
        instruction: instruction_pda,
        creator,
        system_program: solana_program::system_program::ID,
    };

    let args = utils::squads::squads_mpl::client::args::AddInstruction{
        incoming_instruction: utils::squads::squads_mpl::types::IncomingInstruction{
            program_id: incoming_instruction.program_id,
            keys: to_ms_account_meta(&incoming_instruction.accounts),
            data: incoming_instruction.data.clone(),
        }
    };

    let ix_data = args.data();

    Instruction::new_with_bytes(
        utils::squads::squads_mpl::ID,
        ix_data.as_slice(),
        accounts.to_account_metas(None),
    )
}

pub fn activate_transaction_ix(multisig: Pubkey,
                               transaction_pda: Pubkey,
                               creator: Pubkey,) -> Instruction {
    let accounts = utils::squads::squads_mpl::client::accounts::ActivateTransaction{
        multisig,
        transaction: transaction_pda,
        creator,
    };
    let args = utils::squads::squads_mpl::client::args::ActivateTransaction{};
    let data = args.data();
    Instruction::new_with_bytes(
        utils::squads::squads_mpl::ID,
        data.as_slice(),
        accounts.to_account_metas(None),
    )
}

fn to_ms_account_meta(accounts: &Vec<AccountMeta>) -> Vec<utils::squads::squads_mpl::types::MsAccountMeta> {
    accounts.into_iter().map(|account| utils::squads::squads_mpl::types::MsAccountMeta {
        pubkey: account.pubkey,
        is_signer: account.is_signer,
        is_writable: account.is_writable,
    }).collect()
}
