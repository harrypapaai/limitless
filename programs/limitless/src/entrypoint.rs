#![cfg(not(feature = "no-entrypoint"))]

use crate::{errors, processor::Processor};
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult,
    pubkey::Pubkey,
};
use solana_program::program_error::PrintProgramError;
use errors::LimitlessError;

entrypoint!(process_instruction);
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {

    if let Err(e) = Processor::process(program_id, accounts, instruction_data) {
        // Catch the error so we can print it
        e.print::<LimitlessError>();
        return Err(e);
    }
    Ok(())
}