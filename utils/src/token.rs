use std::ops::Deref;
use solana_program::account_info::AccountInfo;
use solana_program::program::{invoke, invoke_signed};
use solana_program::program_pack::Pack;
use solana_program::rent::Rent;
use solana_program::system_instruction;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use spl_token::state::Account as TokenAccount;
use crate::errors::UtilsError;

pub fn amount_from_token_account_info(info: &AccountInfo) -> Result<u64, UtilsError> {
    amount_from_token_account_data(info.data.borrow().deref())
}

pub fn amount_from_token_account_data(data: &[u8]) -> Result<u64, UtilsError> {
    if data.len() < 72 {
        return Err(UtilsError::TokenAccountSerializationFailed);
    }
    let mut amount_bytes = [0u8; 8];
    amount_bytes.copy_from_slice(&data[64..72]);
    Ok(u64::from_le_bytes(amount_bytes))
}

pub fn create_token_account_with_signers<'info>(
    token_account_info: &AccountInfo<'info>,
    token_mint_info: &AccountInfo<'info>,
    owner_info: &AccountInfo<'info>,
    payer_info: &AccountInfo<'info>,
    token_program_info: &AccountInfo<'info>,
    system_program_info: &AccountInfo<'info>,
    rent_info: &AccountInfo<'info>,
    pda_signer_seeds: &[&[u8]],
    owner_pda_seeds: &[&[u8]],
) -> Result<(), solana_program::program_error::ProgramError> {
    let token_account_space = TokenAccount::LEN;
    let token_account_lamports = Rent::default().minimum_balance(token_account_space);

    invoke_signed(
        &system_instruction::create_account(
            payer_info.key,
            token_account_info.key,
            token_account_lamports,
            token_account_space as u64,
            token_program_info.key,
        ),
        &[
            payer_info.clone(),
            owner_info.clone(),
            token_account_info.clone(),
            system_program_info.clone(),
        ],
        &[pda_signer_seeds],
    )?;
    invoke_signed(
        &spl_token::instruction::initialize_account(
            &spl_token::ID,
            token_account_info.key,
            token_mint_info.key,
            owner_info.key,
        )?,
        &[
            token_account_info.clone(),
            token_mint_info.clone(),
            owner_info.clone(),
            token_program_info.clone(),
            rent_info.clone(),
        ],
        &[owner_pda_seeds],
    )?;
    Ok(())
}

pub fn create_token_account_idempotent<'info>(
    token_account_info: &AccountInfo<'info>,
    token_mint_info: &AccountInfo<'info>,
    owner_info: &AccountInfo<'info>,
    payer_info: &AccountInfo<'info>,
    token_program_info: &AccountInfo<'info>,
    system_program_info: &AccountInfo<'info>,
) -> Result<(), solana_program::program_error::ProgramError> {
    invoke(
        &create_associated_token_account_idempotent(
            payer_info.key,
            owner_info.key,
            token_mint_info.key,
            token_program_info.key,
        ),
        &[
            payer_info.clone(),
            token_account_info.clone(),
            owner_info.clone(),
            token_mint_info.clone(),
            system_program_info.clone(),
            token_program_info.clone(),
        ],
    )?;
    Ok(())
}
