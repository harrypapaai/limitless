use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult,
    pubkey::Pubkey,
};
use solana_program::program_error::ProgramError;
use crate::instructions::LimitlessInstruction;

pub struct Processor {}
impl Processor {
    pub fn process(
        _program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        LimitlessInstruction::unpack(instruction_data)
            .map_err(|_| ProgramError::InvalidInstructionData)?
            .process(accounts)
            .map_err(|e| e.into())
    }
}
