use std::num::TryFromIntError;
use derive_more::{Display, Error};

#[derive(Clone, Copy, Display, Debug, PartialEq, Eq, Error)]
pub enum UtilsError {
    InvalidAccountsList,
    ClockGetFailed,

    // Token specific errors.
    TokenAccountSerializationFailed,

    // Raydium specific errors.
    InvalidRaydiumProgramAccount,
    InvalidRaydiumConfigPda,
    InvalidRaydiumPoolStatePda,
    InvalidRaydiumLpMintAccount,
    InvalidRaydiumToken0VaultAccount,
    InvalidRaydiumToken1VaultAccount,
    InvalidRaydiumPoolObservationStatePda,
    InvalidRaydiumPoolAuthorityPda,
    RaydiumPoolStateSerializationFailed,
    RaydiumAmmConfigSerializationFailed,
}

#[derive(Clone, Copy, Display, Debug, PartialEq, Eq, Error)]
pub enum NumberError {
    AdditionOverflow,
    MultiplicationOverflow,
    SubtractionUnderflow,
    DivisionByZero,

    ExceedMaxSupportedDecimals,
    PrecisionLoss,
    SizeOverflow,

    IncompatibleConversion,
}

impl From<TryFromIntError> for NumberError {
    fn from(e: TryFromIntError) -> Self {
        match e {
            _ => NumberError::IncompatibleConversion,
        }
    }
}

