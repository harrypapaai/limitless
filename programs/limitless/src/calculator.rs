use std::cmp::{max, min};
use std::ops::{Neg};
use num::{BigInt, BigUint};
use num_traits::{Signed, ToPrimitive, Zero};
use solana_program::clock::DEFAULT_MS_PER_SLOT;
use utils::log;
use crate::errors::*;
use utils::numbers::{self, SafeUnsigned};

//
// Useful definitions to know:
//
// ----------
//
// Opening a position involves:
// - Borrowing liquidity from the pool
// - Swapping the collateral tokens in the pool for the position token
//
// Closing a position involves:
// - Swapping either collateral tokens or position tokens so that we have the correct ratio of
//   assets
// - Depositing the assets back into the pool to repay the loan
//
// ----------
//
// Given X and Y, the amount of tokens removed from the pool when an LP position is redeemed,
// the expected value of an LP position as a function of price (Y/X) and k (X*Y) is:
//   V(p) = X*p + Y
//        = sqrt(k)/sqrt(p)*p + sqrt(k)*sqrt(p)
//        = 2 * sqrt(k) * sqrt(p)
//

const NOT_ENOUGH_LIQUIDITY_THRESHOLD: u128 = 100;

#[derive(Debug)]
pub struct PositionFees {
    // All fees are collateral token amounts.

    // Sum of fees below.
    pub total_fees: u64,

    // Fee charged by raydium when swapping collateral tokens redeemed from LP tokens
    // to open the position.
    pub raydium_open_swap_fee: u64,
    // Fee charged by raydium when swapping collateral tokens to close the position.
    pub raydium_close_collateral_token_swap_fee: u64,
    // Amount of extra collateral needed to cover loss in position amount due to fee
    // charged by raydium when swapping position tokens to close the position.
    pub raydium_close_position_token_swap_fee: u64,
    // When swapping for position close, we overestimate the target liquidity that needs to
    // be repaid (see raydium_fee_buffer). Additional collateral is required to cover the
    // additional loan repayment that might be paid.
    pub raydium_additional_collateral_buffer_for_close_swap: u64,
    // After removing the LP from the pool and constructing the position, the collateral
    // requirement will be:
    //  (position_tokens_removed * collateral_tokens_removed)
    //   / (position_tokens_removed + swap(collateral_tokens_removed))
    //
    // When computing the collateral requirements, the swap operation will round down and
    // the divide operation will round up.
    //
    // This fee covers that error.
    pub rounding_fee: u64,
}

#[derive(Debug)]
pub struct OpenPositionCalcsResult {
    // The maximum liquidity the user can borrow with the specified collateral and starting pool
    // conditions.
    pub max_liquidity: u64,
    // The amount of collateral tokens to be swapped when opening the position.
    // Amount includes swap fees.
    pub collateral_tokens_to_use_for_open_swap: u64,
    // Fees charged by raydium to open and close the position.
    pub fees: PositionFees,

    // The expected number of position tokens to be removed from the pool if max_liquidity LP
    // tokens are redeemed.
    pub expected_position_tokens_removed: u64,
    // The expected number of collateral tokens to be removed from the pool if max_liquidity LP
    // tokens are redeemed.
    pub expected_collateral_tokens_removed: u64,
    // The expected final size of the position after the collateral tokens redeemed from the
    // LP tokens are swapped.
    pub expected_position_size: u64,
    // Expected position tokens in pool after position open.
    pub expected_pool_position_tokens_after: u64,
    // Expected collateral tokens in pool after position open.
    pub expected_pool_collateral_tokens_after: u64,
}

// When opening a position, we need to determine what the max liquidity the user can borrow is.
//
// This is derived by the max loss the user would experience, which is given by k/T.
// where k = position_tokens_removed * collateral_tokens_removed (when
//   redeeming the borrowed liquidity) and T = the final position size after swapping the
//   redeemed liquidity.
//
//   This can be derived by:
//   - Construct value function for position as function of price of position token (p):
//      = T*p - 2*sqrt(k * p)
//      = V(p)
//   - Find the minimum of the value function:
//      V'(p) = T - sqrt(k) / sqrt(p) [V'(p) == d/dp(V(p))]
//      V'(p) = 0 => p = k / T^2
//      V(k/T^2) = -k/T
//   - Maximum loss is k/T
//
// So if the collateral amount is given by k/T, then the ratio of LP tokens that can be borrowed
// (R) is:
// T = R * X_0 + swap(R * Y_0)
//   = R * X_0 + ((X_0 - R * X_0) * R * Y_0) / (Y_0 - R * Y_0 + R * Y_0)
//   = R * X_0 + (1 - R) * R * X_0
//   = R * X_0 + (2 - R)
//
//   k = R * X_0 * R * Y_0
// k/T = R * X_0 * R * Y_0 / (R * X_0 + (2 - R))
//     = R * Y_0 / (2 - R)
//     = c * Y_0 / (Y_0 + c)
//
// When performing calculations, we calculate fees charged by raydium separately.
// These are charged to the user in addition to the collateral amount.
//
pub fn open_position_calcs(
    // The collateral amount the user is providing to open the position.
    collateral_amt: u64,
    // The initial amount of position tokens in the pool.
    // Should not include protocol and fund fees.
    position_token_pool_amt: u64,
    // The initial amount of collateral tokens in the pool.
    // Should not include protocol and fund fees.
    collateral_token_pool_amt: u64,
    // The total amount of LP tokens for the pool. This is the total supply of the raydium pool,
    // not the LP tokens available to borrow.
    lp_supply: u64,
    // The total amount of LP tokens available to borrow (not the total supply).
    lp_tokens_available: u64,
    // The raydium trade fee, including the protocol and fund fees.
    raydium_trade_fee_rate: u64,
    // Raydium protocol fee.
    raydium_protocol_fee_rate: u64,
    // Raydium fund fee.
    raydium_fund_fee_rate: u64,
) -> Result<OpenPositionCalcsResult, LimitlessError> {
    let collateral_amt_u128 = collateral_amt as u128;
    let position_token_pool_amt_u128 = position_token_pool_amt as u128;
    let collateral_token_pool_amt_u128 = collateral_token_pool_amt as u128;
    let lp_supply_u128 = lp_supply as u128;
    let lp_tokens_available_u128 = lp_tokens_available as u128;
    let collateral_tokens_available = collateral_token_pool_amt_u128
        .safe_mul(lp_tokens_available_u128)?
        .safe_div(lp_supply_u128)?;

    let collateral_buffer_for_raydium_fee = raydium_fee_buffer(
        collateral_amt_u128,
        raydium_trade_fee_rate,
    )?;

    let ratio_num = 2u128.safe_mul(collateral_amt_u128)?;
    let ratio_den = collateral_tokens_available
        .safe_add(collateral_amt_u128)?;
    let max_liquidity = lp_tokens_available_u128
        .safe_mul(ratio_num)?
        .safe_div(ratio_den)?;
    if max_liquidity > lp_tokens_available_u128 {
        log!(
            "NotEnoughLiquidity: collateral results in borrowing more then the available liquidity in the pool. \
            collateral_amt {} collateral_tokens_available {}",
            collateral_amt, collateral_tokens_available,
        );
        return Err(LimitlessError::NotEnoughLiquidity);
    }
    if max_liquidity.is_zero() {
        let err = if lp_tokens_available_u128 > NOT_ENOUGH_LIQUIDITY_THRESHOLD {
            Err(LimitlessError::NotEnoughCollateral)
        } else {
            Err(LimitlessError::NotEnoughLiquidity)
        };
        log!(
            "NotEnoughCollateral/NotEnoughLiquidity: collateral results in a max borrowable liquidity of 0. \
            collateral_amt {} pool_position_token_amt {} pool_collateral_token_amt {}",
            collateral_amt, position_token_pool_amt, collateral_token_pool_amt,
        );
        return err;
    }

    let utils::raydium::curves::TradingTokenResult{
        token_0_amount: position_tokens_removed,
        token_1_amount: collateral_tokens_removed,
    } = utils::raydium::curves::ConstantProductCurve::lp_tokens_to_trading_tokens(
        max_liquidity,
        lp_supply_u128,
        position_token_pool_amt_u128,
        collateral_token_pool_amt_u128,
        utils::raydium::curves::RoundDirection::Floor,
    )?;
    if position_tokens_removed.is_zero() {
        let err = if lp_tokens_available_u128 > NOT_ENOUGH_LIQUIDITY_THRESHOLD {
            Err(LimitlessError::NotEnoughCollateral)
        } else {
            Err(LimitlessError::NotEnoughLiquidity)
        };
        log!("NotEnoughCollateral/NotEnoughLiquidity: max borrowed liquidity redeems 0 position tokens");
        return err;
    }
    if collateral_tokens_removed.is_zero() {
        let err = if lp_tokens_available_u128 > NOT_ENOUGH_LIQUIDITY_THRESHOLD {
            Err(LimitlessError::NotEnoughCollateral)
        } else {
            Err(LimitlessError::NotEnoughLiquidity)
        };
        log!("NotEnoughCollateral/NotEnoughLiquidity: max borrowed liquidity redeems 0 collateral tokens");
        return err;
    }

    let open_swap_input_pre_fee = utils::raydium::fees::calculate_pre_fee_amount(
        collateral_tokens_removed,
        raydium_trade_fee_rate,
    )?;
    let (
        open_swap_output,
        collateral_pool_balance_after,
        position_pool_balance_after
    ) = calculate_swap_result_u128(
        open_swap_input_pre_fee,
        collateral_token_pool_amt_u128.safe_sub(collateral_tokens_removed)?,
        position_token_pool_amt_u128.safe_sub(position_tokens_removed)?,
        raydium_trade_fee_rate,
        raydium_protocol_fee_rate,
        raydium_fund_fee_rate,
    )?;
    if open_swap_output.is_zero() {
        let err = if lp_tokens_available_u128 > NOT_ENOUGH_LIQUIDITY_THRESHOLD {
            Err(LimitlessError::NotEnoughCollateral)
        } else {
            Err(LimitlessError::NotEnoughLiquidity)
        };
        log!("NotEnoughCollateral/NotEnoughLiquidity: swapping redeemed collateral tokens results in 0 position tokens");
        return err;
    }

    let expected_total_position_size = position_tokens_removed.safe_add(open_swap_output)?;

    let open_swap_fee = open_swap_input_pre_fee.safe_sub(collateral_tokens_removed)?;
    let collateral_token_swap_when_closing_fee = utils::raydium::fees::calculate_pre_fee_amount(
        collateral_amt_u128,
        raydium_trade_fee_rate,
    )?.safe_sub(collateral_amt_u128)?;
    let collateral_required_to_cover_position_reduction_from_closing_swap_fee = {
        // Compute how much of the position we will have left over after the swap fee.
        let position_trading_fee = utils::raydium::fees::trading_fee(
            expected_total_position_size,
            raydium_trade_fee_rate,
        )?;
        let position_size_after_fee = expected_total_position_size.safe_sub(position_trading_fee)?;
        // Compute the difference in collateral requirements.
        let collateral_req_before_fee = collateral_requirement_u128(
            expected_total_position_size,
            position_tokens_removed,
            collateral_tokens_removed,
        )?;
        let collateral_req_after_fee = collateral_requirement_u128(
            position_size_after_fee,
            position_tokens_removed,
            collateral_tokens_removed,
        )?;

        collateral_req_after_fee.safe_sub(collateral_req_before_fee)?
    };
    // After removing the LP from the pool and constructing the position, the collateral
    // requirement will be:
    //  (position_tokens_removed * collateral_tokens_removed)
    //   / (position_tokens_removed + swap(collateral_tokens_removed))
    //
    // When computing the collateral requirements, the swap operation will round down and
    // the divide operation will round up.
    //
    // We cover the gap as an additional fee.
    let collateral_requirement = collateral_requirement_u128(
        expected_total_position_size,
        position_tokens_removed,
        collateral_tokens_removed,
    )?;
    let rounding_fee = collateral_requirement.saturating_sub(collateral_amt_u128);
    let total_fees = open_swap_fee
        .safe_add(collateral_token_swap_when_closing_fee)?
        .safe_add(collateral_required_to_cover_position_reduction_from_closing_swap_fee)?
        .safe_add(collateral_buffer_for_raydium_fee)?
        .safe_add(rounding_fee)?;

    Ok(OpenPositionCalcsResult {
        max_liquidity: max_liquidity.try_into()?,
        collateral_tokens_to_use_for_open_swap: open_swap_input_pre_fee.try_into()?,
        fees: PositionFees {
            total_fees: total_fees.try_into()?,
            raydium_open_swap_fee: open_swap_fee.try_into()?,
            raydium_close_collateral_token_swap_fee: collateral_token_swap_when_closing_fee.try_into()?,
            raydium_close_position_token_swap_fee: collateral_required_to_cover_position_reduction_from_closing_swap_fee
                .try_into()?,
            raydium_additional_collateral_buffer_for_close_swap: collateral_buffer_for_raydium_fee.try_into()?,
            rounding_fee: rounding_fee.try_into()?,
        },

        expected_position_tokens_removed: position_tokens_removed.try_into()?,
        expected_collateral_tokens_removed: collateral_tokens_removed.try_into()?,
        expected_position_size: expected_total_position_size.try_into()?,
        expected_pool_position_tokens_after: position_pool_balance_after.try_into()?,
        expected_pool_collateral_tokens_after: collateral_pool_balance_after.try_into()?,
    })
}

pub const BLACKWING_PROTOCOL_FEE: u128 = 200_000;
pub const BLACKWING_FEE_DENOM: u128 = 1_000_000;
pub const MS_PER_YEAR: u128 = 365 * 24 * 60 * 60 * 1000;
pub const MS_PER_SLOT: u128 = DEFAULT_MS_PER_SLOT as u128;
pub const OPTIMAL_UTILIZATION: u128 = 80;
pub const UTILIZATION_DENOM: u128 = 100;

// Computes the amount of quote_token to be paid as a fee for borrowing liquidity.
pub fn blackwing_fee_quote_token_amt(
    liquidity_value_being_borrowed_quote_token: u64,
    liquidity_tokens_being_borrowed: u64,
    duration_slots: u64,
    base_fee_apr: u64,
    total_liquidity_tokens_already_borrowed: u64,
    liquidity_supply: u64,
    min_borrow_cost: u64,
    rollover: bool,
) -> Result<u64, LimitlessError> {
    let liquidity_supply_u128 = liquidity_supply as u128;
    let liquidity_value_being_borrowed_quote_token_u128 = liquidity_value_being_borrowed_quote_token as u128;
    let duration_slots_u128 = duration_slots as u128;
    let base_fee_apr_u128 = base_fee_apr as u128;
    let min_borrow_cost_u128 = min_borrow_cost as u128;

    let yearly_borrow_cost = liquidity_value_being_borrowed_quote_token_u128
        .safe_mul(base_fee_apr_u128)?
        .safe_ceil_div(BLACKWING_FEE_DENOM)?;
    let ms_duration = duration_slots_u128.safe_mul(MS_PER_SLOT)?;
    let mut duration_borrow_cost = yearly_borrow_cost
        .safe_mul(ms_duration)?
        .safe_ceil_div(MS_PER_YEAR)?;
    if duration_borrow_cost < min_borrow_cost_u128 {
        duration_borrow_cost = min_borrow_cost_u128;
    }

    // From 0% -> 80% utilization, will scale linearly until 10x the min_borrow_cost.
    // From 80% -> 100% utilization, will scale linearly until 100x the min_borrow_cost.
    let total_liquidity_tokens_borrowed_after_trade = if !rollover {
        (total_liquidity_tokens_already_borrowed as u128)
            .safe_add(liquidity_tokens_being_borrowed as u128)?
    } else {
        total_liquidity_tokens_already_borrowed as u128
    };
    Ok(if total_liquidity_tokens_borrowed_after_trade*UTILIZATION_DENOM < liquidity_supply_u128*OPTIMAL_UTILIZATION {
        let mut scale = 125u128
            .safe_mul(total_liquidity_tokens_borrowed_after_trade)?
            .safe_div(liquidity_supply_u128)?
            .safe_div(10u128)?;
        if scale < 1 {
            scale = 1;
        }
        duration_borrow_cost.safe_mul(scale)?.try_into()?
    } else {
        let mut scale = 4500u128
            .safe_mul(total_liquidity_tokens_borrowed_after_trade)?
            .safe_div(liquidity_supply_u128)?
            .safe_div(10u128)?
            .safe_sub(350u128)?;
        if scale < 1 {
            scale = 1;
        }
        duration_borrow_cost.safe_mul(scale)?.try_into()?
    })
}

// When closing a position, the amount of tokens needed to be swapped needs to be calculated so
// that the desired inequalities are met (see close_position_calcs).
//
// When swapping, collected fees are automatically added to the pool, impacting the pool price.
// Trying to account for this price impact makes calculations too complex.
//
// Instead, the target loan liquidity (k) that needs to be repaid is adjusted by the max
// increase/decrease in price due to fees. When this adjusted k value is used in calculations,
// it guarantees that the original k value will always be met.
//
// Because this buffer is the upper bound, using it will result in excess repayment. To cover this
// excess, buffer also needs to be added to the collateral.
//
// This function computes the buffer amount for both k and collateral.
//
// Derivation:
//  Let X, Y be the initial amount of token_x and token_y in a pool,
//  x be the amount of token_x being swapped (after fees), y be the amount of token_y swapped for,
//  and f be the fee amount (the % of the trade input amount kept added to the pool after the swap).
//  Note that raydium has a protocol and fund fee which are a % of the trading fee that are kept by
//  the protocol. These can be simplified into a single f value, so we won't explicitly account for
//  them here.
//
//  Price after swap (including fees) is given by:
//  P_i(x)
//  = (Y - y) / (X + x/(1-f)) [post-fee amount * (1-f) == pre-fee amount]
//  = (Y - Y*x/(X+x)) / (X + x/(1-f))
//  = Y*X / ((X+x) * (X + x/(1-f)))
//
//  Price after swap (no fees) is given by:
//  P(x) = (Y - y) / (X + x) = Y*X / (X+x)^2
//
//  P(x)/P_i(x) as x -> inf = (X + x/(1-f)) / (X + x) = 1/(1-f)
//
//  So P(x) * (1-f) = P_i(x).
//
//  With these bounds, we can compute the maximum difference in k between the computed amounts
//  (using the price that doesn't factor in fees) and the actual k (using the price that does
//  factor in fees).
//
//  Let p_actual be the actual price of the pool (including fees).
//
//  p_actual = p * (1-f)
//
//  Computed amounts to deposit:
//   X_D(p) = sqrt(k)/sqrt(p)
//   Y_D(p) = sqrt(k)*sqrt(p)
//
//  k from actual amounts deposited:
//   X_D(p) * X_D(p) * p_actual [because p_actual < p, X_D(p) is used as the controlling amount when depositing LP]
//   = sqrt(k)/sqrt(p) * sqrt(k)/sqrt(p) * p * (1-f)
//   = k * (1-f)
//
//  If we instead increase the target k to be k / (1-f), then:
//
//  Computed amounts to deposit:
//   X_D(p) = sqrt(k / (1-f))/sqrt(p)
//   Y_D(p) = sqrt(k / (1-f))*sqrt(p)
//
//  k from actual amounts deposited:
//   X_D(p) * X_D(p) * p_actual
//   = sqrt(k / (1-f))/sqrt(p) * sqrt(k / (1-f))/sqrt(p) * p * (1-f)
//   = k / (1-f) * (1-f)
//   = k
//
//  We can derive the same logic for when token_y is being swapped instead of token_x.
pub fn raydium_fee_buffer(amt: u128, raydium_trade_fee_rate: u64) -> Result<u128, LimitlessError> {
    // Technically, this should be trading fee - protocol fee - fund fee.
    // But it's ok to overestimate and just use the trading fee.
    let trade_fee = utils::raydium::fees::trading_fee(
        amt,
        raydium_trade_fee_rate,
    )?;
    Ok(trade_fee)
}

// The value of the liquidity borrowed (in quote_token).
pub fn liquidity_value_for_borrow(
    liquidity: u64,
    lp_supply: u64,
    quote_token_pool_amt: u64,
) -> Result<u64, LimitlessError> {
    Ok((liquidity as u128)
        .safe_mul(quote_token_pool_amt as u128)?
        // Round up since this is for a borrow.
        .safe_ceil_div(lp_supply as u128)?
        .safe_mul(2u128)?
        .try_into()?)
}

#[derive(Debug)]
pub struct OpenShortPositionCalcsResult {
    // Amount of collateral in base_token (after swapping the provided quote_token amt).
    pub base_token_collateral_amt: u64,
    // The expected amount of base_token to receive after swapping.
    pub expected_base_token_swap_output: u64,
    // Because swapping to cover the fees changes pool conditions, there will be some excess
    // base_token fees left over.
    pub excess_base_token_fees: u64,
    // Total amount of quote_token to swap.
    pub total_quote_token_to_swap: u64,
    // Amount of fees we need to pay to swap the base_token collateral into quote_token.
    pub quote_token_fees_for_swap: u64,
    // Results from the open_position_calcs function, using `base_token_collateral_amt` collateral
    // and the pool state after swapping for the above.
    pub open_calcs: OpenPositionCalcsResult,
}

pub fn open_short_position_calcs(
    quote_token_collateral: u64,
    // The initial amount of base_token in the pool.
    // Should not include protocol and fund fees.
    open_base_token_pool_amt: u64,
    // The initial amount of quote_token tokens in the pool.
    // Should not include protocol and fund fees.
    open_quote_token_pool_amt: u64,
    // The total amount of LP tokens for the pool. This is the total supply of the raydium pool,
    // not the LP tokens available to borrow.
    lp_supply: u64,
    // The total amount of LP tokens available to borrow (not the total supply).
    lp_tokens_available: u64,
    // The raydium trade fee, including the protocol and fund fees.
    raydium_trade_fee_rate: u64,
    // Raydium protocol fee.
    raydium_protocol_fee_rate: u64,
    // Raydium fund fee.
    raydium_fund_fee_rate: u64,
) -> Result<OpenShortPositionCalcsResult, LimitlessError> {
    let quote_token_collateral_u128 = quote_token_collateral as u128;
    let open_base_token_pool_amt_u128 = open_base_token_pool_amt as u128;
    let open_quote_token_pool_amt_u128 = open_quote_token_pool_amt as u128;

    // First compute the expected amount of base_token the user will get after swapping.
    let collateral_trade_fee = utils::raydium::fees::trading_fee(
        quote_token_collateral_u128,
        raydium_trade_fee_rate,
    )?;
    let collateral_trade_fund_fee = utils::raydium::fees::fund_fee(
        collateral_trade_fee,
        raydium_fund_fee_rate,
    )?;
    let collateral_trade_protocol_fee = utils::raydium::fees::protocol_fee(
        collateral_trade_fee,
        raydium_protocol_fee_rate,
    )?;
    let base_token_collateral_amt = utils::raydium::curves::ConstantProductCurve::swap_base_input_without_fees(
        quote_token_collateral_u128,
        open_quote_token_pool_amt_u128,
        open_base_token_pool_amt_u128
    )?;

    // Perform calculations to get an estimate on fees for base_token collateral.
    let quote_token_pool_amt_after_swap = open_quote_token_pool_amt_u128
        .safe_add(quote_token_collateral_u128)?
        .safe_add(collateral_trade_fee)?
        .safe_sub(collateral_trade_fund_fee)?
        .safe_sub(collateral_trade_protocol_fee)?;
    let base_token_pool_amt_after_swap = open_base_token_pool_amt_u128.safe_sub(base_token_collateral_amt)?;
    let open_calcs_estimate = open_position_calcs(
        base_token_collateral_amt.try_into()?,
        quote_token_pool_amt_after_swap.try_into()?,
        base_token_pool_amt_after_swap.try_into()?,
        lp_supply,
        lp_tokens_available,
        raydium_trade_fee_rate,
        raydium_protocol_fee_rate,
        raydium_fund_fee_rate,
    )?;

    // There will be some additional amount of base_token required to cover fees.
    // Figure out how much quote_token we need to swap for the additional base_token required to cover fees.
    let additional_base_token_required = open_calcs_estimate.fees.total_fees as u128 + 10; // Add 10 to cover rounding error.
    let total_base_token_swap_output = base_token_collateral_amt
        .safe_add(additional_base_token_required)?;
    let total_quote_token_to_swap_post_fee = utils::raydium::curves::ConstantProductCurve::swap_base_output_without_fees(
        total_base_token_swap_output,
        open_quote_token_pool_amt_u128,
        open_base_token_pool_amt_u128,
    )?;
    let actual_base_token_received = utils::raydium::curves::ConstantProductCurve::swap_base_input_without_fees(
        total_quote_token_to_swap_post_fee,
        open_quote_token_pool_amt_u128,
        open_base_token_pool_amt_u128,
    )?;
    let extra_base_token_received = actual_base_token_received
        .safe_sub(total_base_token_swap_output)?;
    let total_quote_token_to_swap = utils::raydium::fees::calculate_pre_fee_amount(
        total_quote_token_to_swap_post_fee,
        raydium_trade_fee_rate,
    )?;
    let total_quote_token_swap_fees = total_quote_token_to_swap
        .safe_sub(total_quote_token_to_swap_post_fee)?;
    let quote_token_protocol_fees = utils::raydium::fees::protocol_fee(
        total_quote_token_swap_fees,
        raydium_protocol_fee_rate,
    )?;
    let quote_token_fund_fees = utils::raydium::fees::fund_fee(
        total_quote_token_swap_fees,
        raydium_fund_fee_rate,
    )?;
    let expected_base_token_amt_after_swap = open_base_token_pool_amt_u128
        .safe_sub(actual_base_token_received)?;
    let expected_quote_token_amt_after_swap = open_quote_token_pool_amt_u128
        .safe_add(total_quote_token_to_swap)?
        .safe_sub(quote_token_fund_fees)?
        .safe_sub(quote_token_protocol_fees)?;
    let open_calcs = open_position_calcs(
        base_token_collateral_amt.try_into()?,
        expected_quote_token_amt_after_swap.try_into()?,
        expected_base_token_amt_after_swap.try_into()?,
        lp_supply,
        lp_tokens_available,
        raydium_trade_fee_rate,
        raydium_protocol_fee_rate,
        raydium_fund_fee_rate,
    )?;
    let excess_base_token_fees = open_calcs_estimate.fees.total_fees
        .safe_add(10)? // To account for the 10 added above.
        .safe_add(extra_base_token_received.try_into()?)?
        .safe_sub(open_calcs.fees.total_fees)?;

    Ok(OpenShortPositionCalcsResult {
        base_token_collateral_amt: base_token_collateral_amt.try_into()?,
        expected_base_token_swap_output: total_base_token_swap_output.try_into()?,
        open_calcs,
        excess_base_token_fees,
        total_quote_token_to_swap: total_quote_token_to_swap.try_into()?,
        quote_token_fees_for_swap: total_quote_token_swap_fees.try_into()?,
    })
}

#[derive(Debug)]
pub struct ClosePositionCalcsResult {
    // The amount of position tokens that should be swapped. Amount includes swap fees.
    pub position_tokens_to_swap: u64,
    // The amount of collateral tokens that should be swapped. Amount includes swap fees.
    pub collateral_tokens_to_swap: u64,
    // The total amount of position tokens that should be deposited to create the LP position.
    pub position_tokens_to_deposit: u64,
    // The total amount of collateral tokens that should be deposited to create the LP position.
    pub collateral_tokens_to_deposit: u64,
    // The number of LP tokens that should be minted.
    pub lp_tokens_minted: u64,
    // The LP position might not be able to be recreated due to rounding - the value of a single LP
    // token is larger than the buffer we set aside. In this case, we re-create as much of the LP
    // position as possible and return the rest as fees.
    pub position_tokens_returned_as_fee: u64,
    // The LP position might not be able to be recreated due to rounding - the value of a single LP
    // token is larger than the buffer we set aside. In this case, we re-create as much of the LP
    // position as possible and return the rest as fees.
    pub collateral_tokens_returned_as_fee: u64,

    // The expected position tokens that should be received after swapping
    // position_tokens_to_swap.
    pub expected_position_tokens_received: u64,
    // The expected collateral tokens that should be received after swapping
    // collateral_tokens_to_swap.
    pub expected_collateral_tokens_received: u64,
    // Estimated amm fees in quote tokens charged to the user.
    pub amm_fees_charged_quote_token: u64,
    // Amount of quote tokens sent to the user (excludes blackwing fees).
    pub quote_tokens_transferred_to_user: u64,
    // Amount of base tokens expected in the pool after the position is closed.
    pub pool_final_base_token_amt: u64,
    // Amount of quote tokens expected in the pool after the position is closed.
    pub pool_final_quote_token_amt: u64,
}

// When closing the position, we need to ensure that the value of the LP position created matches
// the expected value of the original LP position.
//
// To figure out how much (if any) is needed to be swapped, we'll express our requirements
// mathematically.
//
// The value of an LP position in terms of current price (p): V(p) = 2 * sqrt(k) * sqrt(p)
// Value of new LP position created V_n(p) = 2 * sqrt(k_n) * sqrt(p)
//
// This implies that k == k_n -> k = X_D * Y_D must be true, where X_D and Y_D are the amount of
// token_x and token_y deposited.
//
// Expressing X_D and Y_D in terms of the current price (p):
//  X_D(p) = sqrt(k)/sqrt(p)
//  Y_D(p) = sqrt(k)*sqrt(p)
//
// Assume we'll be swapping some amount of token_x, given by x.
//
// Let X_0 and Y_0 be the initial amounts in a pool, and k_0 = X_0 * Y_0.
//
// Price after the swap (no fees) is given by (y is the amount swapped out):
// P(x) = (Y_0 - y) / (X_0 + x)
//      = k_0 / (X_0 + x)^2
//
// Let x be the amount to swap.
//
// We need to ensure that after swapping, we have enough token_x from the position size (T) to
// cover X_D. This is expressed as:
//
// X_D(P(x)) <= T - x
//  -> X_D(k_0 / (X_0 + x)^2) <= T - x
//  -> sqrt(k)/sqrt(k_0 / (X_0 + x)^2) <= T - x
//  -> sqrt(k) * X_0 + sqrt(k) * x <= sqrt(k_0) * T - sqrt(k_0) * x
//  -> x <= (sqrt(k_0) * T - sqrt(k) * X_0) / (sqrt(k) + sqrt(k_0))
//
// We need to also ensure that the amount of collateral (c) plus the swap output is enough
// to cover Y_D. This is expressed as:
//
// Y_D(P(x)) <= c + swap(x)
//  -> Y_D(k_0 / (X_0 + x)^2) <= c + (Y_0 * x) / (X_0 + x)
//  -> sqrt(k) * sqrt(k_0 / (X_0 + x)^2) <= c + (Y_0 * x) / (X_0 + x)
//  -> sqrt(k) * sqrt(k_0) / (X_0 + x) <= c + (Y_0 * x) / (X_0 + x)
//  -> sqrt(k) * sqrt(k_0) <= (c * X_0) + (c * x) + (Y_0 * x)
//  -> (sqrt(k) * sqrt(k_0) - c * X_0) / (c + Y_0) <= x
//
// This establishes an upper and lower bound on x.
//
// If we are swapping y token_y, we can similarly construct bounds on y, yielding:
//
// y <= (sqrt(k_0) * c - sqrt(k) * Y_0) / (sqrt(k) + sqrt(k_0))
// (sqrt(k) * sqrt(k_0) - T * Y_0) / (T + X_0) <= y
//
// As long as the collateral constraint of c >= k/T is true, neither upper and lower bound pairs
// intersect (https://www.desmos.com/calculator/cg9rphcrhc).
//
// To determine how much to swap, we just use the lower bounds. If the x lower bound is negative,
// the y lower bound will be positive (and vice versa).
//
// -------
//
// The price amount described above doesn't include fees. This is covered by a
// buffer (see raydium_fee_buffer)
//
// -------
//
// The maximum fees incurred from swapping when closing (both base_token and quote_token) has already
// been held when the position was opened.
//
// -------
//
// The goal of this function is to be able to always close any opened position with any
// valid pool state.
//
// The one condition it depends on is that if given a pool with initial token
// amounts X and Y and an initial LP token supply of LP,
// the following will always hold for all new token amounts and LP token
// supply amounts (X', Y', LP'):
//
// sqrt(X * Y) / LP <= sqrt(X' * Y') / LP'
//
// Proof:
// - Under swaps, X*Y = k is constant and LP tokens supply is constant.
//   So ratio does not change
// - When random people send tokens to the pool, k = X*Y increases and LP token supply
//   does not change. New ratio is larger than old one.
// - When people deposit or withdraw liquidity:
//
//   Say people deposit X_D and Y_D. We know that (Y + Y_D)/(X + X_D) = Y/X and LP
//   supply increase = (X_D/X * LP)
//
//   So new ratio is:
//    sqrt((X + X_D)*(Y + Y_D))/(LP + (X_D/X * LP))
//    == (sqrt((X + X_D)*(Y + Y_D)) * X)/(LP * (X + X_D))
//    == (X + X_D) * sqrt(Y/X) * X / (LP * (X + X_D))
//    == sqrt(Y/X) * X / LP
//    == sqrt(X*Y) / LP
//
//   Ratio remains the same.
//
pub fn close_position_calcs(
    // The position tokens removed from the pool after redeeming the LP position.
    loan_position_token_amt: u64,
    // The collateral tokens removed from the pool after redeeming the LP position.
    loan_collateral_token_amt: u64,
    // Amount of position tokens making up the position.
    position_size: u64,
    // Amount of collateral tokens backing the position.
    collateral_amt: u64,
    // If the position is short or not.
    is_short: bool,
    // Current number of position tokens in the pool.
    position_token_pool_amt: u64,
    // Current number of collateral tokens in the pool.
    collateral_token_pool_amt: u64,
    // Current LP token supply. This is the total supply of the raydium pool,
    // not the LP tokens available to borrow.
    lp_supply: u64,
    // Raydium trading fee.
    raydium_trade_fee_rate: u64,
    // Raydium protocol fee.
    raydium_protocol_fee_rate: u64,
    // Raydium fund fee.
    raydium_fund_fee_rate: u64,
    // The position token balance of this position (without blackwing fees, but includes amm fees).
    position_token_balance_without_blackwing_fees: u64,
    // The collateral token balance of this position (without blackwing, but includes amm fees).
    collateral_token_balance_without_blackwing_fees: u64,
) -> Result<ClosePositionCalcsResult, LimitlessError> {
    let loan_position_token_amt_u128 = loan_position_token_amt as u128;
    let loan_collateral_token_amt_u128 = loan_collateral_token_amt as u128;
    let position_token_pool_amt_u128 = position_token_pool_amt as u128;
    let collateral_token_pool_amt_u128 = collateral_token_pool_amt as u128;
    let position_size_u128 = position_size as u128;
    let collateral_amt_u128 = collateral_amt as u128;
    let lp_supply_u128 = lp_supply as u128;

    let liquidity_buffer_for_raydium_fee = raydium_fee_buffer(
        loan_collateral_token_amt_u128,
        raydium_trade_fee_rate,
    )?;
    let collateral_buffer_for_raydium_fee = raydium_fee_buffer(
        collateral_amt_u128,
        raydium_trade_fee_rate,
    )?;
    let loan_k = loan_position_token_amt_u128
        .safe_mul(loan_collateral_token_amt_u128)?;

    let loan_k_with_buffer = loan_k.safe_add(liquidity_buffer_for_raydium_fee)?;
    let collateral_amt_with_buffer = collateral_amt_u128.safe_add(collateral_buffer_for_raydium_fee)?;

    let loan_k_with_buffer_sqrt = numbers::sqrt_round_up_u128(loan_k_with_buffer);
    let pool_k_sqrt = numbers::sqrt_round_up_u128(
        position_token_pool_amt_u128.safe_mul(collateral_token_pool_amt_u128)?,
    );
    let a = loan_k_with_buffer_sqrt.safe_mul(pool_k_sqrt)?;

    // Lower bound of how much position token to swap. Includes swap fees.
    let position_tokens_to_swap = {
        let b = collateral_amt_with_buffer.safe_mul(position_token_pool_amt_u128)?;
        if a > b {
            let to_swap = a.safe_sub(b)?
                .safe_ceil_div(collateral_amt_with_buffer.safe_add(collateral_token_pool_amt_u128)?)?;
            utils::raydium::fees::calculate_pre_fee_amount(
                to_swap,
                raydium_trade_fee_rate,
            )?
        } else {
            0
        }
    };
    // Lower bound of how much collateral token to swap. Includes swap fees.
    let collateral_tokens_to_swap = if position_tokens_to_swap == 0{
        let b = position_size_u128.safe_mul(collateral_token_pool_amt_u128)?;
        if a > b {
            let to_swap = a.safe_sub(b)?
                .safe_ceil_div(position_size_u128.safe_add(position_token_pool_amt_u128)?)?;
            utils::raydium::fees::calculate_pre_fee_amount(
                to_swap,
                raydium_trade_fee_rate,
            )?
        } else {
            0
        }
    } else {
        0
    };

    // Emulate swap: compute the output and new pool amounts.
    let (
        position_tokens_received,
        collateral_tokens_received,
        new_pool_position_tokens,
        new_pool_collateral_tokens,
        initial_swap_fees_quote_tokens,
    ) = if position_tokens_to_swap > 0 {
        let (output, new_input_pool_amt, new_output_pool_amt) = calculate_swap_result_u128(
            position_tokens_to_swap,
            position_token_pool_amt_u128,
            collateral_token_pool_amt_u128,
            raydium_trade_fee_rate,
            raydium_protocol_fee_rate,
            raydium_fund_fee_rate,
        )?;
        let quote_token_swap_fees = if !is_short {
            // Is base swap
            calculate_quote_token_swap_fee_for_base_tokens(
                position_token_pool_amt_u128,
                collateral_token_pool_amt_u128,
                raydium_trade_fee_rate,
                position_tokens_to_swap,
            )?
        } else {
            // Is quote swap
            calculate_quote_token_swap_fee_for_quote_tokens(
                raydium_trade_fee_rate,
                position_tokens_to_swap,
            )?
        };
        (0, output, new_input_pool_amt, new_output_pool_amt, quote_token_swap_fees.try_into()?)
    } else if collateral_tokens_to_swap > 0 {
        let (output, new_input_pool_amt, new_output_pool_amt) = calculate_swap_result_u128(
            collateral_tokens_to_swap,
            collateral_token_pool_amt_u128,
            position_token_pool_amt_u128,
            raydium_trade_fee_rate,
            raydium_protocol_fee_rate,
            raydium_fund_fee_rate,
        )?;
        let quote_token_swap_fees = if !is_short {
            // Is quote swap
            calculate_quote_token_swap_fee_for_quote_tokens(
                raydium_trade_fee_rate,
                collateral_tokens_to_swap,
            )?
        } else {
            // Is base swap
            calculate_quote_token_swap_fee_for_base_tokens(
                collateral_token_pool_amt_u128,
                position_token_pool_amt_u128,
                raydium_trade_fee_rate,
                collateral_tokens_to_swap,
            )?
        };
        (output, 0, new_output_pool_amt, new_input_pool_amt, quote_token_swap_fees.try_into()?)
    } else {
        (0, 0, position_token_pool_amt_u128, collateral_token_pool_amt_u128, 0u64)
    };

    // TODO: optimize.
    let new_pool_position_tokens_bi = BigUint::from(new_pool_position_tokens);
    let new_pool_collateral_tokens_bi = BigUint::from(new_pool_collateral_tokens);
    let position_tokens_to_deposit = BigUint::from(loan_k)
        .safe_mul(&new_pool_position_tokens_bi)?
        .safe_div(&new_pool_collateral_tokens_bi)?
        .sqrt()
        .safe_add(&BigUint::from(1u32))?
        .to_u128().ok_or(LimitlessError::NumberErrorIncompatibleConversion)?;
    let collateral_tokens_to_deposit = BigUint::from(loan_k)
        .safe_mul(&new_pool_collateral_tokens_bi)?
        .safe_div(&new_pool_position_tokens_bi)?
        .sqrt()
        .safe_add(&BigUint::from(1u32))?
        .to_u128().ok_or(LimitlessError::NumberErrorIncompatibleConversion)?;
    let computed_k = position_tokens_to_deposit.safe_mul(collateral_tokens_to_deposit)?;
    if computed_k < loan_k {
        log!("InvalidLiquidityLoanRepayment: actual k {} loan k {}", computed_k, loan_k);
        return Err(LimitlessError::LiquidityUnderpayment)
    }

    let position_token_balance = (position_token_balance_without_blackwing_fees as u128)
        .safe_sub(position_tokens_to_swap)?
        .safe_add(position_tokens_received)?;
    let collateral_token_balance = (collateral_token_balance_without_blackwing_fees as u128)
        .safe_sub(collateral_tokens_to_swap)?
        .safe_add(collateral_tokens_received)?;

    if position_token_balance < position_tokens_to_deposit {
        log!(
            "PositionBalanceExceeded: balance {} required {}",
            position_token_balance_without_blackwing_fees,
            position_tokens_to_deposit,
        );
        return Err(LimitlessError::PositionBalanceExceeded)
    }
    if collateral_token_balance < collateral_tokens_to_deposit {
        log!(
            "CollateralBalanceExceeded: balance {} required {}",
            collateral_token_balance_without_blackwing_fees,
            collateral_tokens_to_deposit,
        );
        return Err(LimitlessError::CollateralBalanceExceeded)
    }

    let (
        lp_tokens_rounded_up,
        actual_position_tokens_rounded_up,
        actual_collateral_tokens_rounded_up,
    ) = calculate_lp_token_amt_rounded_up_u128(
        lp_supply_u128,
        new_pool_position_tokens,
        new_pool_collateral_tokens,
        position_tokens_to_deposit,
        collateral_tokens_to_deposit,
    )?;
    let actual_k = actual_position_tokens_rounded_up.safe_mul(actual_collateral_tokens_rounded_up)?;
    if actual_k < loan_k {
        log!("InvalidLiquidityLoanRepayment: actual k {} loan k {}", computed_k, loan_k);
        return Err(LimitlessError::LiquidityUnderpayment);
    }

    // This can happen if the rounding error for depositing LP tokens is too large.
    // In this case, tokens are returned to the pool as fees.
    let (
        position_tokens_deposited,
        collateral_tokens_deposited,
        lp_tokens_minted,
        position_tokens_returned_as_fee,
        collateral_tokens_returned_as_fees,
    ) = if position_token_balance < actual_position_tokens_rounded_up
            || collateral_token_balance < actual_collateral_tokens_rounded_up {

            let (
                lp_tokens_rounded_down,
                actual_position_tokens_rounded_down,
                actual_collateral_tokens_rounded_down,
            ) = calculate_lp_token_amt_rounded_down_u128(
                lp_supply_u128,
                new_pool_position_tokens,
                new_pool_collateral_tokens,
                position_tokens_to_deposit,
                collateral_tokens_to_deposit,
            )?;
            if position_token_balance < actual_position_tokens_rounded_down {
                log!(
                    "PositionBalanceExceeded: balance {} required {}",
                    position_token_balance,
                    actual_position_tokens_rounded_down,
                );
                return Err(LimitlessError::PositionBalanceExceeded);
            }
            if collateral_token_balance < actual_collateral_tokens_rounded_down {
                log!(
                    "CollateralBalanceExceeded: balance {} required {}",
                    collateral_token_balance,
                    actual_collateral_tokens_rounded_down,
                );
                return Err(LimitlessError::CollateralBalanceExceeded);
            }
            (
                actual_position_tokens_rounded_down.try_into()?,
                actual_collateral_tokens_rounded_down.try_into()?,
                lp_tokens_rounded_down.try_into()?,
                position_token_balance
                    .safe_sub(actual_position_tokens_rounded_down)?
                    .try_into()?,
                collateral_token_balance
                    .safe_sub(actual_collateral_tokens_rounded_down)?
                    .try_into()?
            )
        } else {
            (
                actual_position_tokens_rounded_up.try_into()?,
                actual_collateral_tokens_rounded_up.try_into()?,
                lp_tokens_rounded_up.try_into()?,
                0,
                0,
            )
        };

    let pool_position_tokens_after_deposit = new_pool_position_tokens
        .safe_add(position_tokens_deposited)?;
    let pool_collateral_tokens_after_deposit = new_pool_collateral_tokens
        .safe_add(collateral_tokens_deposited)?;

    let remaining_position_tokens = position_token_balance
        .safe_sub(position_tokens_deposited)?
        .safe_sub(position_tokens_returned_as_fee)?;
    let remaining_collateral_tokens = collateral_token_balance
        .safe_sub(collateral_tokens_deposited)?
        .safe_sub(collateral_tokens_returned_as_fees)?;

    let (
        pool_base_tokens_after_deposit,
        pool_quote_tokens_after_deposit,
        remaining_base_tokens,
        remaining_quote_tokens,
    ) = if !is_short {
        (
            pool_position_tokens_after_deposit,
            pool_collateral_tokens_after_deposit,
            remaining_position_tokens,
            remaining_collateral_tokens,
        )
    } else {
        (
            pool_collateral_tokens_after_deposit,
            pool_position_tokens_after_deposit,
            remaining_collateral_tokens,
            remaining_position_tokens,
        )
    };

    let (
        final_swap_quote_output,
        final_swap_fee_quote_tokens,
        pool_final_base_token_amount,
        pool_final_quote_token_amount,
    ) = if remaining_base_tokens > 0 {
        let (
            quote_output,
            pool_final_base_token_amount,
            pool_final_quote_token_amount,
        ) = calculate_swap_result_u128(
            remaining_base_tokens,
            pool_base_tokens_after_deposit,
            pool_quote_tokens_after_deposit,
            raydium_trade_fee_rate,
            raydium_protocol_fee_rate,
            raydium_fund_fee_rate,
        )?;
        if quote_output > 0 {
            let final_swap_fee_quote_tokens = calculate_quote_token_swap_fee_for_base_tokens(
                pool_base_tokens_after_deposit,
                pool_quote_tokens_after_deposit,
                raydium_trade_fee_rate,
                remaining_base_tokens,
            )?;
            (
                quote_output,
                final_swap_fee_quote_tokens.try_into()?,
                pool_final_base_token_amount,
                pool_final_quote_token_amount,
            )
        } else {
            (0, 0u64, pool_base_tokens_after_deposit, pool_quote_tokens_after_deposit)
        }
    } else {
        (0, 0u64, pool_base_tokens_after_deposit, pool_quote_tokens_after_deposit)
    };

    let quote_tokens_transferred: u64 = remaining_quote_tokens
        .safe_add(final_swap_quote_output)?
        .try_into()?;
    let amm_fees_charged_quote_token =  initial_swap_fees_quote_tokens
        .safe_add(final_swap_fee_quote_tokens)?;

    Ok(ClosePositionCalcsResult{
        position_tokens_to_swap: position_tokens_to_swap.try_into()?,
        collateral_tokens_to_swap: collateral_tokens_to_swap.try_into()?,
        position_tokens_to_deposit: position_tokens_deposited.try_into()?,
        collateral_tokens_to_deposit: collateral_tokens_deposited.try_into()?,
        lp_tokens_minted,
        position_tokens_returned_as_fee: position_tokens_returned_as_fee.try_into()?,
        collateral_tokens_returned_as_fee: collateral_tokens_returned_as_fees.try_into()?,
        expected_position_tokens_received: position_tokens_received.try_into()?,
        expected_collateral_tokens_received: collateral_tokens_received.try_into()?,
        amm_fees_charged_quote_token,
        quote_tokens_transferred_to_user: quote_tokens_transferred,
        pool_final_base_token_amt: pool_final_base_token_amount.try_into()?,
        pool_final_quote_token_amt: pool_final_quote_token_amount.try_into()?,
    })
}

// Performs the swap according to the input parameters, returning the output and the new pool
// amounts (accounting for increases due to fees).
pub fn calculate_swap_result_u128(
    swap_amt: u128,
    input_pool_balance: u128,
    output_pool_balance: u128,
    raydium_trade_fee_rate: u64,
    raydium_protocol_fee_rate: u64,
    raydium_fund_fee_rate: u64,
) -> Result<(u128, u128, u128), LimitlessError> {
    let swap_fee = utils::raydium::fees::trading_fee(
        swap_amt,
        raydium_trade_fee_rate,
    )?;
    let output = utils::raydium::curves::ConstantProductCurve::swap_base_input_without_fees(
        swap_amt.safe_sub(swap_fee)?,
        input_pool_balance,
        output_pool_balance,
    )?;
    let protocol_fee = utils::raydium::fees::protocol_fee(
        swap_fee,
        raydium_protocol_fee_rate,
    )?;
    let fund_fee = utils::raydium::fees::fund_fee(
        swap_fee,
        raydium_fund_fee_rate,
    )?;
    Ok((
        output,
        input_pool_balance
            .safe_add(swap_amt)?
            .safe_sub(protocol_fee)?
            .safe_sub(fund_fee)?,
        output_pool_balance.safe_sub(output)?,
    ))
}

pub fn collateral_requirement_u128(
    position_size: u128,
    position_base_token_amt: u128,
    position_quote_token_amt: u128,
) -> Result<u128, LimitlessError> {
    position_base_token_amt
        .safe_mul(position_quote_token_amt)?
        .safe_ceil_div(position_size)
        .map_err(|e| e.into())
}

pub fn calculate_lp_token_amt_rounded_up_u128(
    lp_supply: u128,
    token_0_pool_amt: u128,
    token_1_pool_amt: u128,
    token_0_deposited: u128,
    token_1_deposited: u128,
) -> Result<(u128, u128, u128), LimitlessError> {
    let lp_tokens_rounded_up = max(
    token_0_deposited
        .safe_mul(lp_supply)?
        .safe_ceil_div(token_0_pool_amt)?,
    token_1_deposited
        .safe_mul(lp_supply)?
        .safe_ceil_div(token_1_pool_amt)?,
    );
    let res_rounded_up = utils::raydium::curves::ConstantProductCurve::lp_tokens_to_trading_tokens(
        lp_tokens_rounded_up,
        lp_supply,
        token_0_pool_amt,
        token_1_pool_amt,
        utils::raydium::curves::RoundDirection::Ceiling,
    )?;
    Ok((
        lp_tokens_rounded_up,
        res_rounded_up.token_0_amount,
        res_rounded_up.token_1_amount,
    ))
}

pub fn calculate_lp_token_amt_rounded_down_u128(
    lp_supply: u128,
    token_0_pool_amt: u128,
    token_1_pool_amt: u128,
    token_0_deposited: u128,
    token_1_deposited: u128,
) -> Result<(u128, u128, u128), LimitlessError> {
    let lp_tokens_rounded_down = min(
        token_0_deposited
        .safe_mul(lp_supply)?
        .safe_div(token_0_pool_amt)?,
        token_1_deposited
        .safe_mul(lp_supply)?
        .safe_div(token_1_pool_amt)?,
    );
    let res_rounded_down = utils::raydium::curves::ConstantProductCurve::lp_tokens_to_trading_tokens(
        lp_tokens_rounded_down,
        lp_supply,
        token_0_pool_amt,
        token_1_pool_amt,
        utils::raydium::curves::RoundDirection::Ceiling,
    )?;
    Ok((
        lp_tokens_rounded_down,
        res_rounded_down.token_0_amount,
        res_rounded_down.token_1_amount,
    ))
}

pub fn check_slippage_opening(
    worst_price_num: u64,
    worst_price_den: u64,
    after_base_token_amt: u64,
    after_quote_token_amt: u64,
    is_short: bool,
) -> Result<(), LimitlessError> {
    // Check slippage.
    if worst_price_num > 0 {
        // if long, cannot go above worse price:
        //      price_num / price_den <= worse_price_num / worse_price_den
        // if short, cannot drop below worse price:
        //      price_num / price_den >= worse_price_num / worse_price_den
        let lhs = (after_quote_token_amt as u128).safe_mul(worst_price_den as u128)?;
        let rhs = (after_base_token_amt as u128).safe_mul(worst_price_num as u128)?;
        if !is_short && lhs > rhs {
            log!(
            "InvalidSlippage: max price for long exceeded pool_price {}/{} worse_price {}/{}",
            after_quote_token_amt, after_base_token_amt, worst_price_num, worst_price_den,
        );
            return Err(LimitlessError::SlippageExceeded);
        } else if is_short && lhs < rhs {
            log!(
            "InvalidSlippage: min price for short exceeded pool_price {}/{} worse_price {}/{}",
            after_quote_token_amt, after_base_token_amt, worst_price_num, worst_price_den,
        );
            return Err(LimitlessError::SlippageExceeded);
        }
    } else {
        log!("Slippage check disabled");
    };
    Ok(())
}

pub fn check_slippage_closing(
    worst_price_num: u64,
    worst_price_den: u64,
    after_base_token_amt: u64,
    after_quote_token_amt: u64,
    is_short: bool,
) -> Result<(), LimitlessError> {
    // Check slippage.
    if worst_price_num > 0 {
        // if long, cannot drop below worse price:
        //      price_num / price_den <= worse_price_num / worse_price_den
        // if short, cannot go above worse price:
        //      price_num / price_den >= worse_price_num / worse_price_den
        let lhs = (after_quote_token_amt as u128).safe_mul(worst_price_den as u128)?;
        let rhs = (after_base_token_amt as u128).safe_mul(worst_price_num as u128)?;
        if is_short && lhs > rhs {
            log!(
            "InvalidSlippage: max price for short exceeded pool_price {}/{} worse_price {}/{}",
            after_quote_token_amt, after_base_token_amt, worst_price_num, worst_price_den,
        );
            return Err(LimitlessError::SlippageExceeded);
        } else if !is_short && lhs < rhs {
            log!(
            "InvalidSlippage: min price for long exceeded pool_price {}/{} worse_price {}/{}",
            after_quote_token_amt, after_base_token_amt, worst_price_num, worst_price_den,
        );
            return Err(LimitlessError::SlippageExceeded);
        }
    } else {
        log!("Slippage check disabled");
    };
    Ok(())
}

pub fn prorated_fee(
    open_block: u64,
    block_duration: u64,
    current_block: u64,
    total_fee_amt: u64,
    min_fee_amt: u64,
) -> Result<u64, LimitlessError> {
    Ok(min(
        max(
            total_fee_amt.safe_mul(current_block.safe_sub(open_block)?)?.safe_ceil_div(block_duration)?,
            min_fee_amt,
        ),
        total_fee_amt,
    ))
}

// We estimate how many quote tokens the base token swap is worth by estimating how
// many quote tokens we would have received if we swapped the pre-fee amount.
pub fn calculate_quote_token_swap_fee_for_base_tokens(
    pool_base_token_amt: u128,
    pool_quote_token_amt: u128,
    trade_fee_rate: u64,
    base_token_amt: u128,
) -> Result<u128, LimitlessError> {
    let base_token_pre_fee_amt = utils::raydium::fees::calculate_pre_fee_amount(
        base_token_amt,
        trade_fee_rate,
    )?;
    let quote_token_pre_fee = utils::raydium::curves::ConstantProductCurve::swap_base_input_without_fees(
        base_token_pre_fee_amt,
        pool_base_token_amt,
        pool_quote_token_amt,
    )?;
    let quote_token_post_fee = utils::raydium::curves::ConstantProductCurve::swap_base_input_without_fees(
        base_token_amt as u128,
        pool_base_token_amt,
        pool_quote_token_amt,
    )?;
    Ok(quote_token_pre_fee.safe_sub(quote_token_post_fee)?)
}

pub fn calculate_quote_token_swap_fee_for_quote_tokens(
    trade_fee_rate: u64,
    quote_token_amt: u128,
) -> Result<u128, LimitlessError> {
    let quote_token_pre_fee_amt = utils::raydium::fees::calculate_pre_fee_amount(
        quote_token_amt,
        trade_fee_rate,
    )?;
    Ok(quote_token_pre_fee_amt.safe_sub(quote_token_amt)?)
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct RedepositFeesCalcResult {
    // The amount of quote_token to swap before redepositing.
    pub quote_token_to_swap: u64,
    // The amount of base_token expected to be received after swapping token_1_to_swap.
    pub expected_base_token_received: u64,
    // The amount of base_token to deposit.
    pub base_token_to_deposit: u64,
    // The amount of quote_token to deposit.
    pub quote_token_to_deposit: u64,
    // The number of LP tokens that will be minted.
    pub expected_lp_tokens_minted: u64,
}

pub fn redeposit_fees_calc(
    base_token_amt: u64,
    quote_token_amt: u64,
    pool_base_token_amt: u64,
    pool_quote_token_amt: u64,
    lp_supply: u64,
    raydium_trade_fee_rate: u64,
    raydium_protocol_fee_rate: u64,
    raydium_fund_fee_rate: u64,
) -> Result<RedepositFeesCalcResult, LimitlessError> {
    let base_token_balance_u128 = base_token_amt as u128;
    let quote_token_amt_u128 = quote_token_amt as u128;
    let lp_supply_u128 = lp_supply as u128;
    let base_token_pool_amt_u128 = pool_base_token_amt as u128;
    let quote_token_pool_amt_u128 = pool_quote_token_amt as u128;

    let base_token_amt_big = BigInt::from(base_token_amt);
    let max_swap_fee = utils::raydium::fees::trading_fee(
        quote_token_amt_u128,
        raydium_trade_fee_rate,
    )?;
    let quote_token_amt_after_fees = quote_token_amt_u128.safe_sub(max_swap_fee)?;
    let quote_token_after_fees_amt_big = BigInt::from(quote_token_amt_after_fees);
    let pool_base_token_amt_big = BigInt::from(pool_base_token_amt);
    let pool_quote_token_amt_big = BigInt::from(pool_quote_token_amt);

    let two = BigInt::from(2u8);
    let k = pool_base_token_amt_big.safe_mul(&pool_quote_token_amt_big)?;
    let a = base_token_amt_big.safe_add(&pool_base_token_amt_big)?;
    let b = two
        .safe_mul(&k.safe_add(&pool_quote_token_amt_big.safe_mul(&base_token_amt_big)?)?)?;
    let c = base_token_amt_big
        .safe_mul(&pool_quote_token_amt_big)?
        .safe_sub(
            &pool_quote_token_amt_big
                .safe_mul(&quote_token_after_fees_amt_big)?
                .safe_mul(&pool_base_token_amt_big)?,
        )?;

    let four = BigInt::from(4u8);
    let den = two.safe_mul(&a)?;
    let det = b.safe_mul(&b)?.safe_sub(&four.safe_mul(&a)?.safe_mul(&c)?)?;
    if det.is_negative() {
        return Ok(RedepositFeesCalcResult {
            quote_token_to_swap: 0,
            expected_base_token_received: 0,
            base_token_to_deposit: 0,
            quote_token_to_deposit: 0,
            expected_lp_tokens_minted: 0,
        });
    }
    let sqrt_det = det.sqrt();

    let neg_b = b.clone().neg();
    let quote_token_to_swap = {
        let x1 = neg_b.safe_add(&sqrt_det)?.safe_div(&den)?;
        let x2 = neg_b.safe_sub(&b)?.safe_div(&den)?;
        if x1.gt(&x2) {
            x1
        } else {
            x2
        }
    };

    let quote_token_to_swap_u128 = quote_token_to_swap.to_u128().ok_or(LimitlessError::NumberErrorIncompatibleConversion)?;
    let quote_token_to_swap_pre_fee = utils::raydium::fees::calculate_pre_fee_amount(
        quote_token_to_swap_u128,
        raydium_trade_fee_rate,
    )?;
    let swap_fee = quote_token_to_swap_pre_fee.safe_sub(quote_token_to_swap_u128)?;
    if swap_fee > max_swap_fee || quote_token_to_swap_pre_fee > quote_token_amt_u128 {
        return Err(LimitlessError::FeeBalanceExceeded);
    }
    let (base_token_output, quote_token_pool_amt_post_swap, base_token_pool_amt_post_swap) = calculate_swap_result_u128(
        quote_token_to_swap_u128,
        pool_quote_token_amt as u128,
        pool_base_token_amt as u128,
        raydium_trade_fee_rate,
        raydium_protocol_fee_rate,
        raydium_fund_fee_rate,
    )?;

    let (mut lp_tokens_minted, mut base_token_to_deposit, mut quote_token_to_deposit) = calculate_lp_token_amt_rounded_down_u128(
        lp_supply_u128,
        base_token_pool_amt_post_swap,
        quote_token_pool_amt_post_swap,
        base_token_output.safe_add(base_token_amt as u128)?,
        (quote_token_amt as u128).safe_sub(quote_token_to_swap_u128)?,
    )?;
    let max = u64::MAX as u128;
    if lp_tokens_minted > max {
        let res_rounded_down = utils::raydium::curves::ConstantProductCurve::lp_tokens_to_trading_tokens(
            max,
            lp_supply_u128,
            base_token_pool_amt_post_swap,
            quote_token_pool_amt_post_swap,
            utils::raydium::curves::RoundDirection::Ceiling,
        )?;
        lp_tokens_minted = max;
        base_token_to_deposit = res_rounded_down.token_0_amount;
        quote_token_to_deposit = res_rounded_down.token_1_amount;
    }
    if lp_tokens_minted == 0 {
        return Ok(RedepositFeesCalcResult {
            quote_token_to_swap: 0,
            expected_base_token_received: 0,
            base_token_to_deposit: 0,
            quote_token_to_deposit: 0,
            expected_lp_tokens_minted: 0,
        });
    }
    let (baseline_lp_tokens_minted, _, _) = calculate_lp_token_amt_rounded_down_u128(
        lp_supply_u128,
        base_token_pool_amt_u128,
        quote_token_pool_amt_u128,
        base_token_balance_u128,
        quote_token_amt_u128,
    )?;
    if lp_tokens_minted < baseline_lp_tokens_minted {
        return Ok(RedepositFeesCalcResult {
            quote_token_to_swap: 0,
            expected_base_token_received: 0,
            base_token_to_deposit: 0,
            quote_token_to_deposit: 0,
            expected_lp_tokens_minted: 0,
        });
    }

    return Ok(RedepositFeesCalcResult {
        quote_token_to_swap: quote_token_to_swap_u128.try_into()?,
        expected_base_token_received: base_token_output.try_into()?,
        base_token_to_deposit: base_token_to_deposit.try_into()?,
        quote_token_to_deposit: quote_token_to_deposit.try_into()?,
        expected_lp_tokens_minted: lp_tokens_minted.try_into()?,
    });
}

#[cfg(test)]
mod tests {
    use rand::Rng;
    use super::*;

    // We enforce that the loan k value must be less than u64::MAX. Collateral must also be less than
    // the pool collateral token amount.
    //
    // loan_k = loan_position_token_amt * loan_collateral_token_amt
    //        = R * pool_position_token_amt * R * pool_collateral_token_amt
    //        = pool_position_token_amt * pool_collateral_token_amt * R^2
    //
    // where R is the ratio of the pool borrowed. Note that R is in [0, 1).
    //
    // If collateral is c and `R = 2c / (pool_collateral_token_amt + c)`
    // we can derive the constraint on collateral:
    //
    //    u64::MAX >= pool_position_token_amt * pool_collateral_token_amt * R^2
    // -> u32::MAX >= sqrt(pool_k) * R
    // -> u32::MAX >= sqrt(pool_k) * 2c / (pool_collateral_token_amt + c)
    // -> u32::MAX * pool_collateral_token_amt + u32::MAX * c >= sqrt(pool_k) * 2c
    // -> u32::MAX * pool_collateral_token_amt >= sqrt(pool_k) * 2c - u32::MAX * c
    // -> u32::MAX * pool_collateral_token_amt >= c * (2 * sqrt(pool_k) - u32::MAX)
    // -> u32::MAX * pool_collateral_token_amt / (2 * sqrt(pool_k) - u32::MAX) >= c
    //
    fn max_collateral(pool_position_token_amt: u64, pool_collateral_token_amt: u64) -> u64 {
        let pool_k_sqrt = numbers::sqrt_round_up_u128(pool_position_token_amt as u128 * pool_collateral_token_amt as u128);
        let max_u32 = u32::MAX as u128;
        if pool_k_sqrt <= max_u32 {
            // Because R in [0, 1), `u32::MAX >= sqrt(pool_k) * R` will always hold.
            // So collateral can be any value.
            return min(u64::MAX, pool_collateral_token_amt);
        }
        let num = max_u32 * pool_collateral_token_amt as u128;
        let den = 2 * pool_k_sqrt - max_u32;
        min((num / den) as u64, pool_collateral_token_amt)
    }

    fn min_collateral(lp_supply: u64, pool_collateral_token_amt: u64) -> u64 {
        let min_one_bound = (pool_collateral_token_amt as u128) / (2u128 * lp_supply as u128 - 2);
        if min_one_bound > 0u128{
            min_one_bound as u64
        } else {
            10
        }
    }

    #[test]
    fn calculate_lp_token_amt_rounded_up_uses_all_inputs() {
        fn test_lp_token_amt(
            lp_supply: u128,
            base_token_pool_amt: u128,
            quote_token_pool_amt: u128,
            base_token_deposited: u128,
            quote_token_deposited: u128,
        ) {
            let (_, actual_base_token_deposited, actual_quote_token_deposited) = calculate_lp_token_amt_rounded_up_u128(
                lp_supply,
                base_token_pool_amt,
                quote_token_pool_amt,
                base_token_deposited,
                quote_token_deposited,
            ).unwrap();
            assert!(actual_base_token_deposited >= base_token_deposited);
            assert!(actual_quote_token_deposited >= quote_token_deposited);
        }

        test_lp_token_amt(1000, 100, 10, 2, 1);
        test_lp_token_amt(1003, 100, 12, 1, 1);
        test_lp_token_amt(90450, 15435, 4534, 543, 54543);
        test_lp_token_amt(90450, 15435, 4534, 543543443, 54543);
        // Fuzz test with random valid values
        for _ in 0..100 {
            let lp_supply = random_non_zero_u64();
            let base_token_pool_amt = random_non_zero_u64();
            let quote_token_pool_amt = random_non_zero_u64();
            let max_base_token_deposit = u64::MAX as u128 * base_token_pool_amt as u128 / lp_supply as u128;
            let base_token_deposited = random_in_range_incl_u64(0, max_base_token_deposit as u64);
            let max_quote_token_deposit = u64::MAX as u128 * quote_token_pool_amt as u128 / lp_supply as u128;
            let quote_token_deposited = random_in_range_incl_u64(0, max_quote_token_deposit as u64);
            test_lp_token_amt(
                lp_supply as u128,
                base_token_pool_amt as u128,
                quote_token_pool_amt as u128,
                base_token_deposited as u128,
                quote_token_deposited as u128,
            );
        }
    }

    #[test]
    fn calculate_lp_token_amt_rounded_down_never_uses_all_inputs() {
        fn test_lp_token_amt(
            lp_supply: u128,
            base_token_pool_amt: u128,
            quote_token_pool_amt: u128,
            base_token_deposited: u128,
            quote_token_deposited: u128,
        ) {
            let (lp_tokens, actual_base_token_deposited, actual_quote_token_deposited) = calculate_lp_token_amt_rounded_down_u128(
                lp_supply,
                base_token_pool_amt,
                quote_token_pool_amt,
                base_token_deposited,
                quote_token_deposited,
            ).unwrap();
            assert!(actual_base_token_deposited <= base_token_deposited);
            assert!(actual_quote_token_deposited <= quote_token_deposited);
            if lp_tokens == 0 {
                assert_eq!(actual_base_token_deposited, 0);
                assert_eq!(actual_quote_token_deposited, 0);
            }
            let res = utils::raydium::curves::ConstantProductCurve::lp_tokens_to_trading_tokens(
                lp_tokens,
                lp_supply,
                base_token_pool_amt,
                quote_token_pool_amt,
                utils::raydium::curves::RoundDirection::Ceiling,
            ).unwrap();
            assert_eq!(res.token_0_amount, actual_base_token_deposited);
            assert_eq!(res.token_1_amount, actual_quote_token_deposited);
        }

        test_lp_token_amt(1000, 100, 10, 2, 1);
        test_lp_token_amt(1003, 100, 12, 1, 1);
        test_lp_token_amt(90450, 15435, 4534, 543, 54543);
        test_lp_token_amt(90450, 15435, 4534, 543543443, 54543);
        // Fuzz test with random valid values
        for _ in 0..100 {
            let lp_supply = random_non_zero_u64();
            let base_token_pool_amt = random_non_zero_u64();
            let quote_token_pool_amt = random_non_zero_u64();
            let max_base_token_deposit = u64::MAX as u128 * base_token_pool_amt as u128 / lp_supply as u128;
            let base_token_deposited = random_in_range_incl_u64(0, max_base_token_deposit as u64);
            let max_quote_token_deposit = u64::MAX as u128 * quote_token_pool_amt as u128 / lp_supply as u128;
            let quote_token_deposited = random_in_range_incl_u64(0, max_quote_token_deposit as u64);
            test_lp_token_amt(
                lp_supply as u128,
                base_token_pool_amt as u128,
                quote_token_pool_amt as u128,
                base_token_deposited as u128,
                quote_token_deposited as u128,
            );
        }
    }

    #[test]
    fn open_and_close_calculations_always_repay_loan() {
        for _ in 0..10_000 {
            let initial_lp_supply = random_in_range_incl_u64(100, u64::MAX);
            let initial_lp_tokens_available = random_in_range_incl_u64(1, initial_lp_supply);
            let initial_position_token_pool_amt = random_non_zero_u64();
            let initial_collateral_token_pool_amt = random_in_range_incl_u64(initial_position_token_pool_amt, u64::MAX);
            let max_collateral_amt = max_collateral(initial_position_token_pool_amt, initial_collateral_token_pool_amt);
            let min_collateral_amt = min_collateral(initial_lp_supply, initial_collateral_token_pool_amt);
            let collateral_amt = random_in_range_incl_u64(min_collateral_amt, max_collateral_amt);
            assert!(min_collateral_amt < (initial_collateral_token_pool_amt.saturating_mul(2)));
            let trade_fee_rate = random_in_range_incl_u64(100, 10000);
            let (
                post_position_token_pool_amt,
                post_collateral_token_pool_amt,
                post_lp_supply,
                post_lp_tokens_available,
            ) = compute_valid_post_params(
                initial_position_token_pool_amt,
                initial_collateral_token_pool_amt,
                initial_lp_supply,
            );
            match open_and_close_position(
                collateral_amt,
                initial_position_token_pool_amt,
                initial_collateral_token_pool_amt,
                initial_lp_supply,
                initial_lp_tokens_available,
                post_position_token_pool_amt,
                post_collateral_token_pool_amt,
                post_lp_supply,
                post_lp_tokens_available,
                trade_fee_rate
            ) {
                Ok(_) => {},
                Err(r) => {
                    println!(
                        "failed test params: \
                        collateral_amt {} \
                        initial_position_token_pool_amt {} \
                        initial_collateral_token_pool_amt {} \
                        initial_lp_supply {} \
                        initial_lp_tokens_available {} \
                        post_position_token_pool_amt {} \
                        post_collateral_token_pool_amt {} \
                        post_lp_supply {} \
                        trade_fee_rate {}",
                        collateral_amt,
                        initial_position_token_pool_amt,
                        initial_collateral_token_pool_amt,
                        initial_lp_supply,
                        initial_lp_tokens_available,
                        post_position_token_pool_amt,
                        post_collateral_token_pool_amt,
                        post_lp_supply,
                        trade_fee_rate,
                    );
                    panic!("Unexpected error {:?}", r);
                }
            }
        }
    }

    #[test]
    fn open_short_position_calculations() {
        for _ in 0..10_000 {
            let initial_lp_supply = random_in_range_incl_u64(100, u64::MAX);
            let initial_lp_tokens_available = random_in_range_incl_u64(100, initial_lp_supply);
            let initial_position_token_pool_amt = random_non_zero_u64();
            let initial_collateral_token_pool_amt = random_in_range_incl_u64(initial_position_token_pool_amt, u64::MAX);
            let max_collateral_amt = max_collateral(initial_position_token_pool_amt, initial_collateral_token_pool_amt);
            let min_collateral_amt = min_collateral(initial_lp_supply, initial_collateral_token_pool_amt);
            let collateral_amt = random_in_range_incl_u64(min_collateral_amt, max_collateral_amt);
            assert!(min_collateral_amt < (initial_collateral_token_pool_amt.saturating_mul(2)));
            let trade_fee_rate = random_in_range_incl_u64(100, 10000);
            let protocol_and_fund_fee_rate = random_in_range_incl_u64(0, utils::raydium::fees::FEE_RATE_DENOMINATOR_VALUE as u64);
            let fund_fee_rate = random_in_range_incl_u64(0, protocol_and_fund_fee_rate);
            let protocol_fee_rate = protocol_and_fund_fee_rate - fund_fee_rate;

            match open_short_position_calcs(
                collateral_amt,
                initial_position_token_pool_amt,
                initial_collateral_token_pool_amt,
                initial_lp_supply,
                initial_lp_tokens_available,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
            ) {
                Ok(_) => (),
                Err(r) => {
                    println!(
                        "failed test params: \
                        collateral_amt {} \
                        initial_position_token_pool_amt {} \
                        initial_collateral_token_pool_amt {} \
                        initial_lp_supply {} \
                        initial_lp_tokens_available {} \
                        trade_fee_rate {} \
                        protocol_fee_rate {} \
                        fund_fee_rate {}",
                        collateral_amt,
                        initial_position_token_pool_amt,
                        initial_collateral_token_pool_amt,
                        initial_lp_supply,
                        initial_lp_tokens_available,
                        trade_fee_rate,
                        protocol_fee_rate,
                        fund_fee_rate
                    );
                    panic!("Unexpected error {:?}", r);
                }
            }
        }
    }

    #[test]
    fn test_open_and_close_position_cases() {
        // Specific cases from the fuzz test.
        open_and_close_position(
            520286592,
            17498672931370410878,
            4809454119300897532,
            9852430578251607103,
            9852430578251607103,
            13487811049025969362,
            15390049050028657362,
            14627386973772469413,
            14627386973772469413,
            10_000,
        ).unwrap();
        open_and_close_position(
            2705180103,
            1742956703965275898,
            12972206527605928358,
            17748129604814466876,
            17748129604814466876,
            4480738119517467100,
            15015635604739279539,
            2890179014946912291,
            2890179014946912291,
            10_000,
        ).unwrap();
        open_and_close_position(
            1329945011,
            2955042730515571907,
            12686326631419249862,
            13413508169347213062,
            13413508169347213062,
            12438199262218923290,
            9652494461624075646,
            15570008934052393338,
            15570008934052393338,
            10_000,
        ).unwrap();
        open_and_close_position(
            211677079,
            1610282225207732883,
            10380138748010370762,
            7960373138285356400,
            7960373138285356400,
            11800861761453940467,
            14958435890662604063,
            11462246519271832660,
            11462246519271832660,
            10_000,
        ).unwrap();
        open_and_close_position(
            2803836902,
            631633737978335120,
            17242334340001755273,
            16544804164390325024,
            16544804164390325024,
            6508712255833791117,
            6416300248332473679,
            18008640479196312837,
            18008640479196312837,
            10_000,
        ).unwrap();
        open_and_close_position(
            944388,
            1376858549320268487,
            14770246553372352946,
            13687426739343062892,
            13687426739343062892,
            13660259466509742394,
            7880931906245643806,
            11057994830811597972,
            11057994830811597972,
            10_000,
        ).unwrap();
        open_and_close_position(
            175626243200,
            17380947004446826791,
            17562624320648755986,
            15321785073936627052,
            15321785073936627052,
            11226837903413155131,
            12215806934687827630,
            4339546372802278676,
            4339546372802278676,
            10_000,
        ).unwrap();
        open_and_close_position(
            11930532,
            12408816048848826079,
            13077354201593705896,
            722840469607958502,
            722840469607958502,
            17586813998520206454,
            u64::MAX,
            100,
            100,
            0,
        ).unwrap();
        open_and_close_position(
            11930532,
            12408816048848826079,
            13077354201593705896,
            722840469607958502,
            722840469607958502,
            17586813998520206454,
            18359166046834698952,
            614310797686958,
            614310797686958,
            224,
        ).unwrap();
        open_and_close_position(
            28263919,
            135631496336835330,
            18004502204859634494,
            14967105846776569743,
            14967105846776569743,
            2036877708861473464,
            10023649035149414650,
            12437293183299315121,
            12437293183299315121,
            2603,
        ).unwrap();
        open_and_close_position(
            851413650,
            4108025087933968491,
            16153395625920978460,
            6026783265380691741,
            6026783265380691741,
            18389308203206121424,
            18436418969172016177,
            13354862429808402002,
            13354862429808402002,
            9871,
        ).unwrap();
        open_and_close_position(
            500000000000,
            1000000000,
            1000000000000,
            31622776601,
            31622776601,
            1000000000,
            1000000000000,
            31622776601,
            31622776601,
            10000,
        ).unwrap();
    }

    #[test]
    fn open_position_sequence() {
        let mut lp_supply = u64::MAX;
        let mut token0_amt = u64::MAX;
        let mut token1_amt = 400_000000000;
        let out = open_position_calcs(
            500000000,
            token0_amt,
            token1_amt,
            lp_supply,
            lp_supply-100,
            10_000,
            0,
            0,
        ).unwrap();
        println!("Position size {}", out.expected_position_size);

        lp_supply -= out.max_liquidity;
        token0_amt -= out.expected_position_tokens_removed;
        token1_amt -= out.fees.raydium_open_swap_fee;
        let out = open_position_calcs(
            100_000000000,
            token0_amt,
            token1_amt,
            lp_supply,
            lp_supply-100,
            10_000,
            0,
            0,
        ).unwrap();
        println!("Position size {}", out.expected_position_size);

        lp_supply -= out.max_liquidity;
        token0_amt -= out.expected_position_tokens_removed;
        token1_amt -= out.fees.raydium_open_swap_fee;
        let out = open_position_calcs(
            100_000000000,
            token0_amt,
            token1_amt,
            lp_supply,
            lp_supply-100,
            10_000,
            0,
            0,
        ).unwrap();
        println!("Position size {}", out.expected_position_size);

        lp_supply -= out.max_liquidity;
        token0_amt -= out.expected_position_tokens_removed;
        token1_amt -= out.fees.raydium_open_swap_fee;
        let out = open_position_calcs(
            100_000000000,
            token0_amt,
            token1_amt,
            lp_supply,
            lp_supply-100,
            10_000,
            0,
            0,
        ).unwrap();
        println!("Position size {}", out.expected_position_size);

        lp_supply -= out.max_liquidity;
        token0_amt -= out.expected_position_tokens_removed;
        token1_amt -= out.fees.raydium_open_swap_fee;
        let out = open_position_calcs(
            010000000,
            token0_amt,
            token1_amt,
            lp_supply,
            lp_supply-100,
            10_000,
            0,
            0,
        ).unwrap();
        println!("Position size {}", out.expected_position_size);
    }

    #[test]
    fn open_short_position_cases() {
        open_short_position_calcs(
            3218356532,
            1036379905212299222,
            3091204987900738749,
            2968959376830097275,
            2968959376830097275,
            1475,
            361210,
            528137,
        ).unwrap();
        open_short_position_calcs(
            1817915852,
            12586421383890624467,
            17586836164197300958,
            4257339435062820534,
            4257339435062820534,
            9134,
            2844,
            7391,
        ).unwrap();
        open_short_position_calcs(
            14000000,
            49375399623676546-14141044440351,
            103007891333-10791680,
            71004764361925,
            71085616925378-82838151465,
            2500,
            160000,
            0,
        ).unwrap();
    }

    fn open_and_close_position(
        collateral_amt: u64,
        initial_position_token_pool_amt: u64,
        initial_collateral_token_pool_amt: u64,
        initial_lp_supply: u64,
        initial_lp_tokens_available: u64,
        post_position_token_pool_amt: u64,
        post_collateral_token_pool_amt: u64,
        post_lp_supply: u64,
        _post_lp_tokens_available: u64,
        trade_fee_rate: u64,
    ) -> Result<(OpenPositionCalcsResult, ClosePositionCalcsResult), LimitlessError> {
        let open_res = open_position_calcs(
            collateral_amt,
            initial_position_token_pool_amt,
            initial_collateral_token_pool_amt,
            initial_lp_supply,
            initial_lp_tokens_available,
            trade_fee_rate,
            0,
            0,
        )?;
        let position_swap_fee = utils::raydium::fees::trading_fee(
            open_res.expected_position_size as u128,
            trade_fee_rate,
        )? as u64;
        let close_res = close_position_calcs(
            open_res.expected_position_tokens_removed,
            open_res.expected_collateral_tokens_removed,
            open_res.expected_position_size.safe_sub(position_swap_fee)?,
            collateral_amt.safe_add(open_res.fees.rounding_fee)?,
            false,
            post_position_token_pool_amt,
            post_collateral_token_pool_amt,
            post_lp_supply,
            trade_fee_rate,
            0,
            0,
            open_res.expected_position_size,
            collateral_amt.
                safe_add(open_res.fees.total_fees)?.
                safe_sub(open_res.fees.raydium_open_swap_fee)?,

        )?;
        Ok((open_res, close_res))
    }

    fn compute_valid_post_params(
        initial_pool_base_token_amt: u64,
        initial_pool_quote_token_amt: u64,
        initial_lp_supply: u64,
    ) -> (u64, u64, u64, u64) {
        let initial_k = initial_pool_base_token_amt as u128 * initial_pool_quote_token_amt as u128;
        let initial_k_sqrt = numbers::sqrt_round_up_u128(initial_k);
        // k' <= u128::MAX -> sqrt(k') <= u64::MAX -> LP' <= u64::MAX * LP / sqrt(k)
        let max_post_lp_supply = min(
            u64::MAX as u128 * initial_lp_supply as u128 / initial_k_sqrt,
            u64::MAX as u128,
        ) as u64;
        let post_lp_supply = random_in_range_incl_u64(1, max_post_lp_supply);
        let post_lp_tokens_available = random_in_range_incl_u64(1, post_lp_supply);

        let min_post_k_sqrt = min(
            (initial_k_sqrt * post_lp_supply as u128).div_ceil(initial_lp_supply as u128),
            u64::MAX as u128,
        ) as u64;
        let target_post_k_sqrt = random_in_range_incl_u64(min_post_k_sqrt, u64::MAX);
        let target_post_k = target_post_k_sqrt as u128 * target_post_k_sqrt as u128;
        assert!(target_post_k >= min_post_k_sqrt as u128 * min_post_k_sqrt as u128);
        let min_pool_token_amt = target_post_k.div_ceil(u64::MAX as u128) as u64;
        let max_pool_token_amt = target_post_k_sqrt;
        let post_pool_base_token_amt = random_in_range_incl_u64(min_pool_token_amt, max_pool_token_amt);
        let post_pool_quote_token_amt = target_post_k.div_ceil(post_pool_base_token_amt as u128) as u64;
        assert!(post_pool_quote_token_amt >= post_pool_base_token_amt);
        assert!(post_pool_base_token_amt as u128 * post_pool_quote_token_amt as u128 >= target_post_k);
        (post_pool_base_token_amt, post_pool_quote_token_amt, post_lp_supply.try_into().unwrap(), post_lp_tokens_available)
    }

    #[test]
    fn test_blackwing_fee() {
        // Base case
        let fee = blackwing_fee_quote_token_amt(
            100_000000,
            10,
            1000,
            100000,
            0,
            100,
            100,
            false,
        ).unwrap();
        assert_eq!(fee, 127);

        // Min borrow fee
        let fee = blackwing_fee_quote_token_amt(
            100_000000,
            10,
            10,
            100000,
            0,
            100,
            100,
            false,
        ).unwrap();
        assert_eq!(fee, 100);

        // Utilization scaling factor.
        let fee = blackwing_fee_quote_token_amt(
            100_000000,
            90,
            10,
            100000,
            0,
            100,
            100,
            false,
        ).unwrap();
        assert_eq!(fee, 5500);

        // Utilization scaling factor with previously borrowed tokens, causes scaling
        // factor to increase.
        let fee = blackwing_fee_quote_token_amt(
            100_000000,
            50,
            10,
            100000,
            40,
            100,
            100,
            false,
        ).unwrap();
        assert_eq!(fee, 5500);

        // Utilization scaling factor with previously borrowed tokens, scaling factor already high.
        let fee = blackwing_fee_quote_token_amt(
            100_000000,
            1,
            10,
            100000,
            89,
            100,
            100,
            false,
        ).unwrap();
        assert_eq!(fee, 5500);

        // Utilization scaling factor with previously borrowed tokens,
        // rollover (so no change to scaling).
        let fee = blackwing_fee_quote_token_amt(
            100_000000,
            50,
            10,
            100000,
            40,
            100,
            100,
            true,
        ).unwrap();
        assert_eq!(fee, 500);
    }

    #[test]
    fn test_redeposit_fees_calcs() {
        // Successful swap possible.
        let res = redeposit_fees_calc(
            0,
            1_000000000,
            1000000_000000000,
            10_000000000,
            1000000000,
            10_000,
            0,
            0,
        ).unwrap();
        assert_eq!(res, RedepositFeesCalcResult {
            quote_token_to_swap: 483320084,
            expected_base_token_received: 45663738318581,
            base_token_to_deposit: 45663738032281,
            quote_token_to_deposit: 501613112,
            expected_lp_tokens_minted: 47848688
        });
        // Successful swap not possible.
        let res = redeposit_fees_calc(
            0,
            10,
            1000000_000000000,
            10000_000000000,
            1000000000,
            10_000,
            0,
            0,
        ).unwrap();
        assert_eq!(res, RedepositFeesCalcResult {
            quote_token_to_swap: 0,
            expected_base_token_received: 0,
            base_token_to_deposit: 0,
            quote_token_to_deposit: 0,
            expected_lp_tokens_minted: 0,
        });

        // Fuzz test
        for _ in 0..10_000 {
            let lp_supply = random_non_zero_u64();
            let token_0_pool_amt = random_in_range_incl_u64(0, u32::MAX as u64);
            let token_1_pool_amt = random_in_range_incl_u64(0, u32::MAX as u64);
            let token_0_amt = random_in_range_incl_u64(0, u32::MAX as u64);
            let token_1_amt = random_in_range_incl_u64(0, u32::MAX as u64);
            let trade_fee_rate = random_in_range_incl_u64(0, utils::raydium::fees::FEE_RATE_DENOMINATOR_VALUE as u64);
            let total_protocol_rate = random_in_range_incl_u64(0, utils::raydium::fees::FEE_RATE_DENOMINATOR_VALUE as u64);
            let protocol_fee_rate = random_in_range_incl_u64(0, total_protocol_rate);
            let fund_fee_rate = total_protocol_rate - protocol_fee_rate;

            let res = redeposit_fees_calc(
                token_0_amt,
                token_1_amt,
                token_0_pool_amt,
                token_1_pool_amt,
                lp_supply,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
            );
            match res {
                Err(e) => {
                    println!("failed test params: token_0_amt: {} token_1_amt: {} token_0_pool_amt: {} \
                    token_1_pool_amt: {} lp_supply: {} trade_fee_rate: {} protocol_fee_rate: {} fund_fee_rate: {}",
                        token_0_amt,
                        token_1_amt,
                        token_0_pool_amt,
                        token_1_pool_amt,
                        lp_supply,
                        trade_fee_rate,
                        protocol_fee_rate,
                        fund_fee_rate,
                    );
                    panic!("Unexpected error {:?}", e);
                }
                Ok(_) => {}
            }
        }
    }

    fn random_non_zero_u64() -> u64 {
        let out = rand::random::<u64>();
        if out == 0 {
            1
        } else {
            out
        }
    }

    fn random_in_range_incl_u64(start: u64, end: u64) -> u64 {
        rand::thread_rng().gen_range(start as u128..end as u128 + 1).try_into().unwrap()
    }
}
