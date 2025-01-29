use std::convert::Infallible;
use std::num::TryFromIntError;
use derive_more::{Display, Error};
use num_derive::{ToPrimitive, FromPrimitive};
use solana_program::decode_error::DecodeError;
use solana_program::msg;
use solana_program::program_error::{PrintProgramError, ProgramError};
use utils::errors::UtilsError;

/// Errors that may be returned by the program.
#[derive(Clone, Copy, Display, Debug, Eq, Error, PartialEq, ToPrimitive, FromPrimitive)]
pub enum LimitlessError {
    // 0
    InvalidInstruction,
    InvalidTokenPairOrder,
    DerivePdaError,

    // Liquidity / Collateral errors.
    NotEnoughLiquidity,
    NotEnoughCollateral,

    // Invalid input errors.
    PositionDoesNotExist,
    MarketDoesNotExist,
    LiquidityPositionAccountDoesNotExist,
    MarketClosed,
    InvalidDurationSlots,
    // 10
    SlippageExceeded,
    BelowMinLeverage,
    InvalidShareAmt,
    ShareAmtTooLow,
    PositionIdAlreadyUsed,
    MarketAlreadyExists,
    MaxRolloverFeeExceeded,
    MaxBlackwingFeeExceeded,
    MaxRaydiumFeeExceeded,
    InsufficientReserveBalanceToRollover,

    // Account validation errors.
    // 20
    InvalidAdmin,
    InvalidSigner,
    InvalidCloser,
    InvalidFeeCollectorAta,
    InvalidTokenProgramAccount,
    InvalidToken2022ProgramAccount,
    InvalidAssociatedTokenProgramAccount,
    InvalidSplMemoProgramAccount,
    InvalidSystemProgramAccount,
    InvalidRentAccount,
    // 30
    InvalidToken0MintAccount,
    InvalidToken1MintAccount,
    InvalidMarketAccountPda,
    InvalidLimitlessConfigAccountPda,
    InvalidPositionAccountPda,
    InvalidMarketToken0Ata,
    InvalidMarketToken1Ata,
    InvalidMarketIntermediateTa,
    InvalidMarketRaydiumLpTokenAta,
    InvalidLiquidityPositionAccountPda,
    // 40
    InvalidUserToken0Ata,
    InvalidUserToken1Ata,
    InvalidUserRaydiumLpTokenAta,
    InvalidEventAuthority,
    InvalidProgramAccount,
    InvalidConfigAccountPda,

    // Account serialization errors.
    MarketStateSerializationFailed,
    PositionStateSerializationFailed,
    PositionAccountSerializationFailed,
    MarketAccountSerializationFailed,
    // 50
    ConfigAccountSerializationFailed,

    // CPI errors.
    LiquidityPositionAccountSerializationFailed,
    WithdrawLpInvokeFailed,
    DepositLpInvokeFailed,
    SwapInvokeFailed,
    LpTokenMintInvokeFailed,
    SplTransferInstructionSerializationFailed,
    SplTransferInvokeFailed,
    WsolMintFailed,
    CreateMarketAccountInvokeFailed,
    // 60
    CreatePositionAccountInvokeFailed,
    CreateLiquidityPositionAccountInvokeFailed,
    CreateTokenAccountInvokeFailed,
    CreateConfigAccountInvokeFailed,
    EmitCpiEventFailed,

    // Calculation discrepancy errors.
    InvalidLpTokensRemoved,
    InvalidBaseTokenRemoved,
    InvalidQuoteTokenRemoved,
    InvalidLpTokensReceived,
    InvalidBaseTokenOutput,
    // 70
    InvalidBaseTokenInput,
    InvalidQuoteTokenOutput,
    InvalidQuoteTokenInput,
    InvalidToken0Output,
    InvalidToken1Output,
    InvalidToken0Input,
    InvalidToken1Input,
    InvalidPositionTokenOutput,
    InvalidCollateralTokenOutput,
    InvalidPositionTokenInput,
    // 80
    InvalidCollateralTokenInput,
    InvalidPositionSize,
    FeeBalanceExceeded,
    PositionBalanceExceeded,
    CollateralBalanceExceeded,

    // Accounting validation errors.
    InvalidBaseTokenBalanceAtEnd,
    InvalidQuoteTokenBalanceAtEnd,
    LiquidityUnderpayment,

    // Number Errors.
    NumberErrorAdditionOverflow,
    NumberErrorMultiplicationOverflow,
    // 90
    NumberErrorSubtractionUnderflow,
    NumberErrorDivisionByZero,
    NumberErrorExceedMaxSupportedDecimals,
    NumberErrorPrecisionLoss,
    NumberErrorSizeOverflow,
    NumberErrorIncompatibleConversion,
    // Utils errors.
    UtilsErrorInvalidAccountsList,
    UtilsErrorRaydiumPoolStateSerializationFailed,
    UtilsErrorRaydiumAmmConfigSerializationFailed,
    UtilsErrorTokenAccountSerializationFailed,
    // 100
    UtilsErrorInvalidRaydiumProgramAccount,
    UtilsErrorInvalidRaydiumConfigPda,
    UtilsErrorInvalidRaydiumPoolStatePda,
    UtilsErrorInvalidRaydiumPoolAuthorityPda,
    UtilsErrorInvalidRaydiumPoolObservationStatePda,
    UtilsErrorInvalidRaydiumLpMintAccount,
    UtilsErrorInvalidRaydiumToken0VaultAccount,
    UtilsErrorInvalidRaydiumToken1VaultAccount,
    UtilsErrorClockGetFailed,

    #[cfg(feature = "localnet")]
    InvalidTokenMintAccount,
    #[cfg(feature = "localnet")]
    InvalidTokenAta,

}

impl<T> DecodeError<T> for LimitlessError {
    fn type_of() -> &'static str {
        "LimitlessError"
    }
}

impl From<LimitlessError> for ProgramError {
    fn from(e: LimitlessError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl PrintProgramError for LimitlessError {
    fn print<E>(&self)
    where
        E: 'static
        + std::error::Error
        + DecodeError<E>
        + PrintProgramError
        + num_traits::FromPrimitive,
    {
        msg!("{}", self)
    }
}

impl From<utils::errors::NumberError> for LimitlessError {
    fn from(e: utils::errors::NumberError) -> Self {
        match e {
            utils::errors::NumberError::AdditionOverflow => LimitlessError::NumberErrorAdditionOverflow,
            utils::errors::NumberError::MultiplicationOverflow => LimitlessError::NumberErrorMultiplicationOverflow,
            utils::errors::NumberError::SubtractionUnderflow => LimitlessError::NumberErrorSubtractionUnderflow,
            utils::errors::NumberError::DivisionByZero => LimitlessError::NumberErrorDivisionByZero,
            utils::errors::NumberError::ExceedMaxSupportedDecimals => LimitlessError::NumberErrorExceedMaxSupportedDecimals,
            utils::errors::NumberError::PrecisionLoss => LimitlessError::NumberErrorPrecisionLoss,
            utils::errors::NumberError::SizeOverflow => LimitlessError::NumberErrorSizeOverflow,
            utils::errors::NumberError::IncompatibleConversion => LimitlessError::NumberErrorIncompatibleConversion,
        }
    }
}

impl From<UtilsError> for LimitlessError {
    fn from(e: UtilsError) -> Self {
        match e {
            UtilsError::InvalidAccountsList => LimitlessError::UtilsErrorInvalidAccountsList,
            UtilsError::TokenAccountSerializationFailed => LimitlessError::UtilsErrorTokenAccountSerializationFailed,
            UtilsError::InvalidRaydiumProgramAccount => LimitlessError::UtilsErrorInvalidRaydiumProgramAccount,
            UtilsError::InvalidRaydiumConfigPda => LimitlessError::UtilsErrorInvalidRaydiumConfigPda,
            UtilsError::InvalidRaydiumPoolStatePda => LimitlessError::UtilsErrorInvalidRaydiumPoolStatePda,
            UtilsError::InvalidRaydiumLpMintAccount => LimitlessError::UtilsErrorInvalidRaydiumLpMintAccount,
            UtilsError::InvalidRaydiumToken0VaultAccount => LimitlessError::UtilsErrorInvalidRaydiumToken0VaultAccount,
            UtilsError::InvalidRaydiumToken1VaultAccount => LimitlessError::UtilsErrorInvalidRaydiumToken1VaultAccount,
            UtilsError::InvalidRaydiumPoolObservationStatePda => LimitlessError::UtilsErrorInvalidRaydiumPoolObservationStatePda,
            UtilsError::InvalidRaydiumPoolAuthorityPda => LimitlessError::UtilsErrorInvalidRaydiumPoolAuthorityPda,
            UtilsError::RaydiumPoolStateSerializationFailed => LimitlessError::UtilsErrorRaydiumPoolStateSerializationFailed,
            UtilsError::RaydiumAmmConfigSerializationFailed => LimitlessError::UtilsErrorRaydiumAmmConfigSerializationFailed,
            UtilsError::ClockGetFailed => LimitlessError::UtilsErrorClockGetFailed,
        }
    }
}

impl From<TryFromIntError> for LimitlessError {
    fn from(e: TryFromIntError) -> Self {
        match e {
            _ => LimitlessError::NumberErrorIncompatibleConversion,
        }
    }
}

impl From<Infallible> for LimitlessError {
    fn from(_e: Infallible) -> Self {
        LimitlessError::NumberErrorIncompatibleConversion
    }
}
