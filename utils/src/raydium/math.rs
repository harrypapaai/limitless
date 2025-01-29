use crate::errors::NumberError;
use crate::numbers::SafeUnsigned;

pub fn ceil_div(token_amount: u128, fee_numerator: u128, fee_denominator: u128) -> Result<u128, NumberError> {
    token_amount
        .safe_mul(u128::from(fee_numerator))?
        .safe_add(fee_denominator)?
        .safe_sub(1)?
        .safe_div(fee_denominator)
}

pub fn floor_div(token_amount: u128, fee_numerator: u128, fee_denominator: u128) -> Result<u128, NumberError> {
    Ok(
        token_amount
            .safe_mul(fee_numerator)?
            .safe_div(fee_denominator)?,
    )
}

pub trait RaydiumSafeCeilDiv: Sized {
    /// Perform ceiling division
    fn raydium_safe_ceil_div(&self, rhs: Self) -> Result<(Self, Self), NumberError>;
}

impl RaydiumSafeCeilDiv for u128 {
    fn raydium_safe_ceil_div(&self, mut rhs: Self) -> Result<(Self, Self), NumberError> {
        let mut quotient = self.safe_div(rhs)?;
        // Avoid dividing a small number by a big one and returning 1, and instead
        // fail.
        if quotient == 0 {
            if self.safe_mul(2)? >= rhs {
                return Ok((1, 0));
            } else {
                return Ok((0, 0));
            }
        }

        // Ceiling the destination amount if there's any remainder, which will
        // almost always be the case.
        let remainder = self.safe_rem(rhs)?;
        if remainder > 0 {
            quotient = quotient.safe_add(1)?;
            // calculate the minimum amount needed to get the dividend amount to
            // avoid truncating too much
            rhs = self.safe_div(quotient)?;
            let remainder = self.safe_rem(quotient)?;
            if remainder > 0 {
                rhs = rhs.safe_add(1)?;
            }
        }
        Ok((quotient, rhs))
    }
}
