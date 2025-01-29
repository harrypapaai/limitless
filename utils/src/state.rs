use std::cell::Ref;
use anchor_lang::prelude::borsh::{
    BorshDeserialize as AnchorBorshDeserialize,
};
use solana_program::program::invoke_signed;
use solana_program::rent::Rent;
use solana_program::system_instruction;
use original_borsh::{BorshDeserialize, BorshSerialize};
use solana_program::clock::Clock;
use solana_program::sysvar::Sysvar;
use crate::errors::UtilsError;

pub trait Packable: BorshSerialize + BorshDeserialize {
    fn pack(&self, account_info: &solana_program::account_info::AccountInfo)
        -> Result<(), solana_program::program_error::ProgramError>;
    fn serialize_vec(&self) -> Result<Vec<u8>, solana_program::program_error::ProgramError>;
    fn unpack(account_info: &solana_program::account_info::AccountInfo)
        -> Result<Self, solana_program::program_error::ProgramError>;
}

pub fn unpack_info<T: BorshDeserialize>(
    account_info: &solana_program::account_info::AccountInfo
) -> Result<T, solana_program::program_error::ProgramError> {
    if account_info.data_is_empty() {
        return Err(solana_program::program_error::ProgramError::UninitializedAccount);
    }
    let data = &mut &account_info.data.borrow()[..];
    T::deserialize(data)
        .map_err(|_| solana_program::program_error::ProgramError::InvalidAccountData)
}

pub fn pack_info<T: BorshSerialize>(
    s: T,
    account_info: &solana_program::account_info::AccountInfo
) -> Result<(), solana_program::program_error::ProgramError> {

    let dst = &mut &mut account_info.data.borrow_mut()[..];
    s.serialize(dst).map_err(|_| solana_program::program_error::ProgramError::InvalidAccountData)
}

pub fn anchor_unpack_info<T: AnchorBorshDeserialize>(
    account_info: &solana_program::account_info::AccountInfo
) -> Result<T, solana_program::program_error::ProgramError> {
    if account_info.data_is_empty() {
        return Err(solana_program::program_error::ProgramError::UninitializedAccount);
    }
    let data = &mut &account_info.data.borrow()[8..];
    T::deserialize(data)
        .map_err(|_| solana_program::program_error::ProgramError::InvalidAccountData)
}

pub fn anchor_unpack_data_ref<T: AnchorBorshDeserialize>(
    data_ref: &Ref<'_, &mut [u8]>,
) -> Result<T, solana_program::program_error::ProgramError> {
    let data = &mut &data_ref[8..];
    T::deserialize(data)
        .map_err(|_| solana_program::program_error::ProgramError::InvalidAccountData)
}

pub fn copy_packed_info(
    data: &Vec<u8>,
    account_info: &solana_program::account_info::AccountInfo
) -> Result<(), solana_program::program_error::ProgramError> {
    let dst = &mut account_info.data.borrow_mut()[..];
    if dst.len() != data.len() {
        return Err(solana_program::program_error::ProgramError::InvalidAccountData);
    }
    dst.clone_from_slice(data);
    Ok(())
}

pub fn create_account_with_signers<'info, T: Packable>(
    data: &T,
    data_info: &solana_program::account_info::AccountInfo<'info>,
    payer_info: &solana_program::account_info::AccountInfo<'info>,
    rent_info: &solana_program::account_info::AccountInfo<'info>,
    system_program_info: &solana_program::account_info::AccountInfo<'info>,
    program_id: &solana_program::pubkey::Pubkey,
    signer_seeds: &[&[&[u8]]],
) -> Result<(), solana_program::program_error::ProgramError> {

    let serialized = data.serialize_vec()?;
    let size = serialized.len();
    let rent_lamports = Rent::default().minimum_balance(size);
    invoke_signed(
        &system_instruction::create_account(
            payer_info.key,
            data_info.key,
            rent_lamports,
            size as u64,
            program_id,
        ),
        &[
            payer_info.clone(),
            data_info.clone(),
            rent_info.clone(),
            system_program_info.clone(),
        ],
        signer_seeds,
    )?;
    copy_packed_info(&serialized, data_info)?;
    Ok(())
}

pub fn clock() ->  Result<Clock, UtilsError> {
    Clock::get().map_err(|_| UtilsError::ClockGetFailed)
}
