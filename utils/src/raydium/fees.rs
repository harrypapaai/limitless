use crate::errors::NumberError;
use crate::numbers::SafeUnsigned;
use crate::raydium::math::{self};
use crate::raydium::raydium_cp_swap::accounts::PoolState;

pub const FEE_RATE_DENOMINATOR_VALUE: u128 = 1_000_000;

/// Calculate the trading fee in trading tokens
pub fn trading_fee(amount: u128, trade_fee_rate: u64) -> Result<u128, NumberError> {
    math::ceil_div(
        amount,
        u128::from(trade_fee_rate),
        FEE_RATE_DENOMINATOR_VALUE,
    )
}

/// Calculate the owner trading fee in trading tokens
pub fn protocol_fee(amount: u128, protocol_fee_rate: u64) -> Result<u128, NumberError> {
    math::floor_div(
        amount,
        u128::from(protocol_fee_rate),
        u128::from(FEE_RATE_DENOMINATOR_VALUE),
    )
}

/// Calculate the owner trading fee in trading tokens
pub fn fund_fee(amount: u128, fund_fee_rate: u64) -> Result<u128, NumberError> {
    math::floor_div(
        amount,
        u128::from(fund_fee_rate),
        u128::from(FEE_RATE_DENOMINATOR_VALUE),
    )
}

pub fn calculate_pre_fee_amount(post_fee_amount: u128, trade_fee_rate: u64) -> Result<u128, NumberError> {
    if trade_fee_rate == 0 {
        Ok(post_fee_amount)
    } else {
        let numerator = post_fee_amount.safe_mul(FEE_RATE_DENOMINATOR_VALUE)?;
        let denominator =
            FEE_RATE_DENOMINATOR_VALUE.safe_sub(u128::from(trade_fee_rate))?;

        numerator
            .safe_add(denominator)?
            .safe_sub(1)?
            .safe_div(denominator)
    }
}

pub fn vault_amount_without_fee(pool_state: &PoolState, vault_0: u64, vault_1: u64) -> Result<(u64, u64), NumberError> {
    let token_0_amt = vault_0
        .safe_sub(pool_state.protocol_fees_token0.safe_add(pool_state.fund_fees_token0)?)?;
    let token_1_amt = vault_1
        .safe_sub(pool_state.protocol_fees_token1 + pool_state.fund_fees_token1)?;
    Ok((token_0_amt, token_1_amt))
}
