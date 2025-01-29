use crate::errors::NumberError;
use crate::numbers::SafeUnsigned;
use crate::raydium::math::{RaydiumSafeCeilDiv};

/// The direction to round.  Used for pool token to trading token conversions to
/// avoid losing value on any deposit or withdrawal.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoundDirection {
    /// Floor the value, ie. 1.9 => 1, 1.1 => 1, 1.5 => 1
    Floor,
    /// Ceiling the value, ie. 1.9 => 2, 1.1 => 2, 1.5 => 2
    Ceiling,
}

/// Encodes results of depositing both sides at once
#[derive(Debug, PartialEq)]
pub struct TradingTokenResult {
    /// Amount of token A
    pub token_0_amount: u128,
    /// Amount of token B
    pub token_1_amount: u128,
}

/// ConstantProductCurve struct implementing CurveCalculator
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConstantProductCurve;

impl ConstantProductCurve {
    /// Constant product swap ensures x * y = constant
    /// The constant product swap calculation, factored out of its class for reuse.
    ///
    /// This is guaranteed to work for all values such that:
    ///  - 1 <= swap_source_amount * swap_destination_amount <= u128::MAX
    ///  - 1 <= source_amount <= u64::MAX
    pub fn swap_base_input_without_fees(
        source_amount: u128,
        swap_source_amount: u128,
        swap_destination_amount: u128,
    ) -> Result<u128, NumberError> {
        // (x + delta_x) * (y - delta_y) = x * y
        // delta_y = (delta_x * y) / (x + delta_x)
        let numerator = source_amount.safe_mul(swap_destination_amount)?;
        let denominator = swap_source_amount.safe_add(source_amount)?;
        let destinsation_amount_swapped = numerator.safe_div(denominator)?;
        Ok(destinsation_amount_swapped)
    }

    pub fn swap_base_output_without_fees(
        destinsation_amount: u128,
        swap_source_amount: u128,
        swap_destination_amount: u128,
    ) -> Result<u128, NumberError> {
        // (x + delta_x) * (y - delta_y) = x * y
        // delta_x = (x * delta_y) / (y - delta_y)
        let numerator = swap_source_amount.safe_mul(destinsation_amount)?;
        let denominator = swap_destination_amount
            .safe_sub(destinsation_amount)?;
        let (source_amount_swapped, _) = numerator.raydium_safe_ceil_div(denominator)?;
        Ok(source_amount_swapped)
    }

    /// Get the amount of trading tokens for the given amount of pool tokens,
    /// provided the total trading tokens and supply of pool tokens.
    ///
    /// The constant product implementation is a simple ratio calculation for how
    /// many trading tokens correspond to a certain number of pool tokens
    pub fn lp_tokens_to_trading_tokens(
        lp_token_amount: u128,
        lp_token_supply: u128,
        swap_token_0_amount: u128,
        swap_token_1_amount: u128,
        round_direction: RoundDirection,
    ) -> Result<TradingTokenResult, NumberError> {
        let mut token_0_amount = lp_token_amount
            .safe_mul(swap_token_0_amount)?
            .safe_div(lp_token_supply)?;
        let mut token_1_amount = lp_token_amount
            .safe_mul(swap_token_1_amount)?
            .safe_div(lp_token_supply)?;
        let (token_0_amount, token_1_amount) = match round_direction {
            RoundDirection::Floor => (token_0_amount, token_1_amount),
            RoundDirection::Ceiling => {
                let token_0_remainder = lp_token_amount
                    .safe_mul(swap_token_0_amount)?
                    .safe_rem(lp_token_supply)?;
                // Also check for 0 token A and B amount to avoid taking too much
                // for tiny amounts of pool tokens.  For example, if someone asks
                // for 1 pool token, which is worth 0.01 token A, we avoid the
                // ceiling of taking 1 token A and instead return 0, for it to be
                // rejected later in processing.
                if token_0_remainder > 0 && token_0_amount > 0 {
                    token_0_amount += 1;
                }
                let token_1_remainder = lp_token_amount
                    .safe_mul(swap_token_1_amount)?
                    .safe_rem(lp_token_supply)?;
                if token_1_remainder > 0 && token_1_amount > 0 {
                    token_1_amount += 1;
                }
                (token_0_amount, token_1_amount)
            }
        };
        Ok(TradingTokenResult {
            token_0_amount,
            token_1_amount,
        })
    }
}
