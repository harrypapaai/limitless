use solana_program::account_info::AccountInfo;
use crate::errors::UtilsError;

pub mod raydium;
pub mod validate;
pub mod errors;
pub mod token;
pub mod numbers;
pub mod state;
pub mod instructions;
pub mod events;
pub mod squads;

#[inline]
pub fn next_account_info<'slice, 'info: 'slice>(
    iter: &mut std::slice::Iter<'slice, AccountInfo<'info>>,
) -> Result<&'slice AccountInfo<'info>, UtilsError> {
    iter.next().ok_or(UtilsError::InvalidAccountsList)
}

#[macro_export]
macro_rules! log {
    ($msg:expr) => {
        solana_program::msg!($msg);
    };
    ($msg:expr, $($arg:tt)*) => {
        solana_program::msg!($msg, $($arg)*);
    };
}
