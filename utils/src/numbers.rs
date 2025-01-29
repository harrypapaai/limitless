use std::fmt::Debug;
use std::ops::Rem;
use derive_more::Display;
use num::{BigInt, BigUint};
use num_traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedRem, CheckedSub, One, Unsigned, Zero};
use crate::errors::*;

pub trait SafeUnsigned<T, Other> {
    fn safe_mul(&self, other: Other) -> Result<T, NumberError>;
    fn safe_div(&self, other: Other) -> Result<T, NumberError>;
    fn safe_ceil_div(&self, other: Other) -> Result<T, NumberError>;
    fn safe_add(&self, other: Other) -> Result<T, NumberError>;
    fn safe_sub(&self, other: Other) -> Result<T, NumberError>;
    fn safe_rem(&self, other: Other) -> Result<T, NumberError>;
}

impl<T: Copy + Zero + One + CheckedAdd + CheckedMul + CheckedDiv + CheckedSub + CheckedRem + Ord + Unsigned> SafeUnsigned<T, T> for T {
    fn safe_mul(&self, other: T) -> Result<T, NumberError> {
        self.checked_mul(&other).ok_or(NumberError::MultiplicationOverflow)
    }

    fn safe_div(&self, other: T) -> Result<T, NumberError> {
        self.checked_div(&other).ok_or(NumberError::DivisionByZero)
    }

    fn safe_ceil_div(&self, other: T) -> Result<T, NumberError> {
        let quotient = self.safe_div(other)?;
        let remainder = self.safe_rem(other)?;
        if remainder > T::zero() {
            Ok(quotient + T::one())
        } else {
            Ok(quotient)
        }
    }

    fn safe_add(&self, other: T) -> Result<T, NumberError> {
        self.checked_add(&other).ok_or(NumberError::AdditionOverflow)
    }

    fn safe_sub(&self, other: T) -> Result<T, NumberError> {
        self.checked_sub(&other).ok_or(NumberError::SubtractionUnderflow)
    }

    fn safe_rem(&self, other: T) -> Result<T, NumberError> {
        self.checked_rem(&other).ok_or(NumberError::DivisionByZero)
    }
}

impl SafeUnsigned<BigUint, &BigUint> for BigUint {
    fn safe_mul(&self, other: &BigUint) -> Result<BigUint, NumberError> {
        self.checked_mul(other).ok_or(NumberError::MultiplicationOverflow)
    }

    fn safe_div(&self, other: &BigUint) -> Result<BigUint, NumberError> {
        self.checked_div(other).ok_or(NumberError::DivisionByZero)
    }

    fn safe_ceil_div(&self, other: &BigUint) -> Result<BigUint, NumberError> {
        let quotient = self.safe_div(other)?;
        let remainder = self.safe_rem(other)?;
        if remainder > BigUint::zero() {
            Ok(quotient + BigUint::one())
        } else {
            Ok(quotient)
        }
    }

    fn safe_add(&self, other: &BigUint) -> Result<BigUint, NumberError> {
        self.checked_add(other).ok_or(NumberError::AdditionOverflow)
    }

    fn safe_sub(&self, other: &BigUint) -> Result<BigUint, NumberError> {
        self.checked_sub(other).ok_or(NumberError::SubtractionUnderflow)
    }

    fn safe_rem(&self, other: &BigUint) -> Result<BigUint, NumberError> {
        Ok(self.rem(other))
    }
}

impl SafeUnsigned<BigInt, &BigInt> for BigInt {
    fn safe_mul(&self, other: &BigInt) -> Result<BigInt, NumberError> {
        self.checked_mul(other).ok_or(NumberError::MultiplicationOverflow)
    }

    fn safe_div(&self, other: &BigInt) -> Result<BigInt, NumberError> {
        self.checked_div(other).ok_or(NumberError::DivisionByZero)
    }

    fn safe_ceil_div(&self, other: &BigInt) -> Result<BigInt, NumberError> {
        let quotient = self.safe_div(other)?;
        let remainder = self.safe_rem(other)?;
        if remainder > BigInt::zero() {
            Ok(quotient + BigInt::one())
        } else {
            Ok(quotient)
        }
    }

    fn safe_add(&self, other: &BigInt) -> Result<BigInt, NumberError> {
        self.checked_add(other).ok_or(NumberError::AdditionOverflow)
    }

    fn safe_sub(&self, other: &BigInt) -> Result<BigInt, NumberError> {
        self.checked_sub(other).ok_or(NumberError::SubtractionUnderflow)
    }

    fn safe_rem(&self, other: &BigInt) -> Result<BigInt, NumberError> {
        Ok(self.rem(other))
    }
}

macro_rules! mul {
    ($a:expr, $b:expr) => {
        $a.checked_mul($b).ok_or(NumberError::MultiplicationOverflow)?
    };
}

macro_rules! div {
    ($a:expr, $b:expr) => {
        $a.checked_div($b).ok_or(NumberError::DivisionByZero)?
    };
}

macro_rules! add {
    ($a:expr, $b:expr) => {
        $a.checked_add($b).ok_or(NumberError::AdditionOverflow)?
    };
}

macro_rules! sub {
    ($a:expr, $b:expr) => {
        $a.checked_sub($b).ok_or(NumberError::SubtractionUnderflow)?
    }
}

macro_rules! check_max {
    ($a:expr) => {
        {
            if $a > u64::MAX as u128 {
                Err(NumberError::SizeOverflow)
            } else {
                Ok($a)
            }
        }
    }
}

// Get square root of `y`.
// Using the Babylonian method (https://en.wikipedia.org/wiki/Methods_of_computing_square_roots#Babylonian_method)
pub fn sqrt_u128(y: u128) -> u128 {
    if y < 4 {
        if y == 0 {
            0u128
        } else {
            1u128
        }
    } else {
        let mut z = y;
        let mut x = y / 2 + 1;
        while x < z {
            z = x;
            x = (y / x + x) / 2; // Dividing by two shouldn't cause any problems.
        };
        z
    }
}

pub fn sqrt_round_up_u128(y: u128) -> u128 {
    let res = sqrt_u128(y);
    if res * res < y {
        res + 1
    } else {
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqrt_u128() {
        assert_eq!(sqrt_u128(0), 0);
        assert_eq!(sqrt_u128(2), 1);
        assert_eq!(sqrt_u128(4), 2);
        assert_eq!(sqrt_u128(16), 4);
        assert_eq!(sqrt_u128(22500), 150);
        assert_eq!(sqrt_u128(10), 3);
    }

    #[test]
    fn test_sqrt_round_up_u128() {
        assert_eq!(sqrt_round_up_u128(0), 0);
        assert_eq!(sqrt_round_up_u128(2), 2);
        assert_eq!(sqrt_round_up_u128(4), 2);
        assert_eq!(sqrt_round_up_u128(16), 4);
        assert_eq!(sqrt_round_up_u128(22500), 150);
        assert_eq!(sqrt_round_up_u128(10), 4);
    }
}

//
// FixedPoint64
//

pub mod fp64 {
    use super::*;

    /// Fixedpoint struct. Can be stored, copied, and dropped.
    #[derive(Display, Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FixedPoint64 {
        // Stored internally as a u128 to prevent overflow during operations.
        val: u128,
    }

    /// Number of decimal places in a FixedPoint value.
    // Must be even to support the sqrt function.
    pub const DECIMAL_PLACES: u8 = 10;

    // Internal enums for modes different operations can be in.
    const MODE_ROUND_UP: u8 = 0;
    const MODE_TRUNCATE: u8 = 1;
    const MODE_NO_PRECISION_LOSS: u8 = 2;

    /// Create a new FixedPoint from a u64 value. No conversion is performed.
    /// Example: `new_u64(12345) == 0.0000012345`
    pub const fn new_u64(val: u64) -> FixedPoint64 {
        FixedPoint64 { val: val as u128 }
    }

    /// Create a new FixedPoint from a u128 value. No conversion is performed.
    /// Example: `new_u128(12345) == 0.0000012345`
    pub fn new_u128(val: u128) -> Result<FixedPoint64, NumberError> {
        Ok(FixedPoint64 { val: check_max!(val)? })
    }

    /// Returns the underlying value of the FixedPoint.
    pub fn value(a: FixedPoint64) -> u128 {
        a.val
    }

    /// Return a FixedPoint that equals 0.
    pub fn zero() -> FixedPoint64 {
        FixedPoint64 { val: 0 }
    }

    /// Return a FixedPoint that equals 1.
    pub fn one() -> FixedPoint64 {
        FixedPoint64 { val: exp(DECIMAL_PLACES) }
    }

    /// Return a FixedPoint that equals 2.
    pub fn two() -> FixedPoint64 {
        FixedPoint64 { val: one().val * 2 }
    }

    /// Return a FixedPoint that equals 0.5.
    pub fn half() -> FixedPoint64 {
        FixedPoint64 { val: one().val / 2 }
    }

    /// Returns the max FixedPoint value.
    pub fn max_fp() -> FixedPoint64 {
        FixedPoint64 { val: u64::MAX as u128 }
    }

    /// Returns the min FixedPoint value.
    pub fn min_fp() -> FixedPoint64 {
        FixedPoint64 { val: 0 }
    }

    /// Converts the value with the specified decimal places to a FixedPoint value.
    pub fn from_u64(v: u64, decimals: u8) -> Result<FixedPoint64, NumberError> {
        from_u128(v as u128, decimals)
    }

    /// Converts the value with the specified decimal places to a FixedPoint value.
    pub fn from_u128(v: u128, decimals: u8) -> Result<FixedPoint64, NumberError> {
        if decimals > DECIMAL_PLACES {
            return Err(NumberError::ExceedMaxSupportedDecimals);
        }

        let int_part = v / exp(decimals); // Dividing by exp(decimals) shouldn't panic.
        let decimals_part = v % exp(decimals); // ^ Ditto.
        let decimal_mult = exp(DECIMAL_PLACES);
        let val = add!(
            mul!(int_part, decimal_mult),
            mul!(decimals_part, exp(DECIMAL_PLACES - decimals))
        );
        Ok(FixedPoint64 { val: check_max!(val)? })
    }

    /// Returns max(a, b).
    pub fn max(a: FixedPoint64, b: FixedPoint64) -> FixedPoint64 {
        if a.val >= b.val {
            a
        } else {
            b
        }
    }

    /// Returns min(a, b).
    pub fn min(a: FixedPoint64, b: FixedPoint64) -> FixedPoint64 {
        if a.val < b.val {
            a
        } else {
            b
        }
    }

    impl FixedPoint64 {
        /// Returns a FixedPoint truncated to the given decimal places.
        pub fn trunc_to_decimals(self, decimals: u8) -> Result<FixedPoint64, NumberError> {
            from_u128(self.to_u128_internal(decimals, MODE_TRUNCATE)?, decimals)
        }

        /// Returns a FixedPoint rounded up to the given decimal places.
        pub fn round_up_to_decimals(self, decimals: u8) -> Result<FixedPoint64, NumberError> {
            from_u128(self.to_u128_internal(decimals, MODE_ROUND_UP)?, decimals)
        }

        /// Converts the FixedPoint to a u64 value with the given number of decimal places.
        /// Truncates any digits that are lost.
        pub fn to_u64_trunc(self, decimals: u8) -> Result<u64, NumberError> {
            let converted = self.to_u128_internal(decimals, MODE_TRUNCATE)?;
            Ok(converted as u64) // Should never overflow because of checks in to_u128_internal.
        }

        /// Converts the FixedPoint to a u128 value with the given number of decimal places.
        /// Truncates any digits that are lost.
        pub fn to_u128_trunc(self, decimals: u8) -> Result<u128, NumberError> {
            self.to_u128_internal(decimals, MODE_TRUNCATE)
        }

        /// Converts the FixedPoint to a u64 value with the given number of decimal places.
        /// Rounds up if digits are lost.
        pub fn to_u64_round_up(self, decimals: u8) -> Result<u64, NumberError> {
            let converted = self.to_u128_internal(decimals, MODE_ROUND_UP)?;
            Ok(converted as u64) // Should never overflow because of checks in to_u128_internal.
        }

        /// Converts the FixedPoint to a u128 value with the given number of decimal places.
        /// Rounds up if digits are lost.
        pub fn to_u128_round_up(self, decimals: u8) -> Result<u128, NumberError> {
            self.to_u128_internal(decimals, MODE_ROUND_UP)
        }

        /// Converts the FixedPoint to a u64 value with the given number of decimal places.
        /// Errors if any digits are lost.
        pub fn to_u64(self, decimals: u8) -> Result<u64, NumberError> {
            let converted = self.to_u128_internal(decimals, MODE_NO_PRECISION_LOSS)?;
            Ok(converted as u64) // Should never overflow because of checks in to_u128_internal.
        }

        /// Converts the FixedPoint to a u128 value with the given number of decimal places.
        /// Errors if any digits are lost.
        pub fn to_u128(self, decimals: u8) -> Result<u128, NumberError> {
            self.to_u128_internal(decimals, MODE_NO_PRECISION_LOSS)
        }

        /// Converts the FixedPoint to a u64 value with the given number of decimal places.
        fn to_u128_internal(self, decimals: u8, mode: u8) -> Result<u128, NumberError> {
            if decimals > DECIMAL_PLACES {
                return Err(NumberError::ExceedMaxSupportedDecimals);
            }

            let decimal_mult = exp(DECIMAL_PLACES);
            let decimal_mult_adj = exp(DECIMAL_PLACES - decimals);

            let int_part = div!(self.val, decimal_mult);
            let decimal_part = div!((self.val % decimal_mult), decimal_mult_adj);

            let mut val = add!(mul!(int_part, exp(decimals)), decimal_part);
            let precision_loss = mul!(decimal_part, decimal_mult_adj) < self.val % decimal_mult;
            if mode == MODE_NO_PRECISION_LOSS && precision_loss {
                return Err(NumberError::PrecisionLoss);
            } else if mode == MODE_ROUND_UP && precision_loss {
                val = add!(val, 1);
            } else if mode == MODE_TRUNCATE {
                // No need to do anything.
            };
            check_max!(val)
        }

        /// Multiplies two FixedPoints, truncating if the number of decimal places exceeds DECIMAL_PLACES.
        pub fn multiply_trunc(self, b: FixedPoint64) -> Result<FixedPoint64, NumberError> {
            let val = div!(mul!(self.val, b.val), exp(DECIMAL_PLACES));
            Ok(FixedPoint64 { val: check_max!(val)? })
        }

        /// Multiplies two FixedPoints, rounding up if the number of decimal places exceeds DECIMAL_PLACES.
        pub fn multiply_round_up(self, b: FixedPoint64) -> Result<FixedPoint64, NumberError> {
            let decimal_mult = exp(DECIMAL_PLACES);
            let mut val = div!(mul!(self.val, b.val), decimal_mult);
            if mul!(val, decimal_mult) < mul!(self.val, b.val) {
                val = add!(val, 1);
            };
            Ok(FixedPoint64 { val: check_max!(val)? })
        }

        /// Divides two FixedPoints, truncating if the number of decimal places exceeds DECIMAL_PLACES.
        pub fn divide_trunc(self, b: FixedPoint64) -> Result<FixedPoint64, NumberError> {
            let val = div!(mul!(self.val, exp(DECIMAL_PLACES)), b.val);
            Ok(FixedPoint64 { val: check_max!(val)? })
        }

        /// Divides two FixedPoints, rounding up if the number of decimal places exceeds DECIMAL_PLACES.
        pub fn divide_round_up(self, b: FixedPoint64) -> Result<FixedPoint64, NumberError> {
            let decimal_mult = exp(DECIMAL_PLACES);
            let mut val = div!(mul!(self.val, decimal_mult), b.val);
            if mul!(val, b.val) < mul!(self.val, decimal_mult) {
                val = add!(val, 1);
            };
            Ok(FixedPoint64 { val: check_max!(val)? })
        }

        /// Returns the approximation of the square root of the FixedPoint using the
        /// [Babylonian method](https://en.wikipedia.org/wiki/Methods_of_computing_square_roots#Babylonian_method).
        /// The approximation will always be less than the actual square root.
        pub fn sqrt_approx(self) -> Result<FixedPoint64, NumberError> {
            let val = self.val;
            let mut sqrt_val = sqrt_u128(val);
            if mul!(sqrt_val, sqrt_val) > val {
                sqrt_val = sub!(sqrt_val, 1);
            };
            from_u128(sqrt_val, DECIMAL_PLACES/2)
        }

        /// Adds two FixedPoints.
        pub fn add(self, b: FixedPoint64) -> Result<FixedPoint64, NumberError> {
            Ok(FixedPoint64 { val: check_max!(add!(self.val, b.val))? })
        }

        /// Subtracts two FixedPoints.
        pub fn sub(self, b: FixedPoint64) -> Result<FixedPoint64, NumberError> {
            Ok(FixedPoint64 { val: check_max!(sub!(self.val, b.val))? })
        }

        /// Return true if a < b.
        pub fn lt(self, b: FixedPoint64) -> bool {
            self.val < b.val
        }

        /// Return true if a <= b.
        pub fn lte(self, b: FixedPoint64) -> bool {
            self.val <= b.val
        }

        /// Return true if a > b.
        pub fn gt(self, b: FixedPoint64) -> bool {
            self.val > b.val
        }

        /// Return true if a >= b.
        pub fn gte(self, b: FixedPoint64) -> bool {
            self.val >= b.val
        }

        /// Return true if a == b.
        pub fn eq(self, b: FixedPoint64) -> bool {
            self.val == b.val
        }

        /// Return true if the value is zero.
        pub fn is_zero(self) -> bool {
            self.val == 0
        }
    }

    // Exponents.
    const F0 : u128 = 1;
    const F1 : u128 = 10;
    const F2 : u128 = 100;
    const F3 : u128 = 1000;
    const F4 : u128 = 10000;
    const F5 : u128 = 100000;
    const F6 : u128 = 1000000;
    const F7 : u128 = 10000000;
    const F8 : u128 = 100000000;
    const F9 : u128 = 1000000000;
    const F10: u128 = 10000000000;
    const F11: u128 = 100000000000;
    const F12: u128 = 1000000000000;
    const F13: u128 = 10000000000000;
    const F14: u128 = 100000000000000;
    const F15: u128 = 1000000000000000;
    const F16: u128 = 10000000000000000;
    const F17: u128 = 100000000000000000;
    const F18: u128 = 1000000000000000000;
    const F19: u128 = 10000000000000000000;
    const F20: u128 = 100000000000000000000;

    // Programmatic way to get a power of 10.
    pub const fn exp(e: u8) -> u128 {
        assert!(e <= 20, "Exceeded max power of 10 allowed");

        if e == 0 {
            F0
        } else if e == 1 {
            F1
        } else if e == 2 {
            F2
        } else if e == 3 {
            F3
        } else if e == 4 {
            F4
        } else if e == 5 {
            F5
        } else if e == 5 {
            F5
        } else if e == 6 {
            F6
        } else if e == 7 {
            F7
        } else if e == 8 {
            F8
        } else if e == 9 {
            F9
        } else if e == 10 {
            F10
        } else if e == 11 {
            F11
        } else if e == 12 {
            F12
        } else if e == 13 {
            F13
        } else if e == 14 {
            F14
        } else if e == 15 {
            F15
        } else if e == 16 {
            F16
        } else if e == 17 {
            F17
        } else if e == 18 {
            F18
        } else if e == 19 {
            F19
        } else if e == 20 {
            F20
        } else {
            0
        }
    }

    #[test]
    fn test_from_to_integer() {
        let input = from_u64(1, 0).unwrap();
        assert_eq!(input.val, exp(DECIMAL_PLACES));
        let converted = input.to_u64( 6).unwrap();
        assert_eq!(converted, 1000000);
    }

    #[test]
    fn test_zero() {
        let zero = zero();
        assert_eq!(zero.val, 0);
    }

    #[test]
    fn test_one() {
        let one = one();
        assert_eq!(one.val, 10000000000);
    }

    #[test]
    fn test_half() {
        let half = half();
        assert_eq!(half.val, 5000000000);
    }

    #[test]
    fn test_from_large_integer() {
        assert_eq!(from_u128(u128::MAX, 10).unwrap_err(), NumberError::SizeOverflow);
    }

    #[test]
    fn test_from_to_zero() {
        let input = from_u64(0, 5).unwrap();
        assert_eq!(input.val, 0);
        let converted = input.to_u64(6).unwrap();
        assert_eq!(converted, 0);
    }

    #[test]
    fn test_from_to_decimals_increase() {
        let input = from_u64(101, 1).unwrap();
        assert_eq!(input.val, 101 * exp(DECIMAL_PLACES - 1));
        let converted = input.to_u64(6).unwrap();
        assert_eq!(converted, 10100000);
    }

    #[test]
    fn test_from_to_decimals_decrease_lose_precision() {
        let input = from_u64(100000001, 10).unwrap();
        assert_eq!(input.val, 100000001 * exp(DECIMAL_PLACES - 10));
        assert_eq!(input.to_u64(6).unwrap_err(), NumberError::PrecisionLoss);
    }

    #[test]
    fn test_from_to_decimals_decrease_lose_precision_round_up() {
        let input = from_u64(100000001, 10).unwrap();
        assert_eq!(input.val, 100000001 * exp(DECIMAL_PLACES - 10));
        assert_eq!(input.to_u64_round_up(6).unwrap(), 10001);
    }

    #[test]
    fn test_from_to_decimals_decrease_lose_precision_truncate() {
        let input = from_u64(100000001, 10).unwrap();
        assert_eq!(input.val, 100000001 * exp(DECIMAL_PLACES - 10));
        assert_eq!(input.to_u64_trunc(6).unwrap(), 10000);
    }

    #[test]
    fn test_multiply() {
        let a = from_u64(1056, 0).unwrap();
        let b = from_u64(2056, 0).unwrap();
        let product = a.multiply_trunc(b).unwrap();
        assert_eq!(product.to_u64(0).unwrap(), 2171136);
    }

    #[test]
    fn test_multiply_with_decimals() {
        let a = from_u64(1056, 3).unwrap();
        let b = from_u64(2056, 1).unwrap();
        let product = a.multiply_trunc(b).unwrap();
        assert_eq!(product.to_u64_trunc(0).unwrap(), 217);
    }

    #[test]
    fn test_multiply_round_up() {
        let a = from_u64(1, 10).unwrap();
        let b = from_u64(1, 10).unwrap();
        let mut product = a.multiply_trunc(b).unwrap();
        assert_eq!(product.to_u64(0).unwrap(), 0);
        product = a.multiply_round_up(b).unwrap();
        assert_eq!(product.to_u64(10).unwrap(), 1);
    }

    #[test]
    fn test_square_root_of_max() {
        // Note, isn't the exact root of MAX_U64.
        let a = from_u128(4294967295, 5).unwrap();
        let product = a.multiply_trunc(a).unwrap();
        assert_eq!(product.to_u128(10).unwrap(), 18446744065119617025);
    }

    #[test]
    fn test_divide_trunc() {
        let a = from_u64(1056, 0).unwrap();
        let b = from_u64(2056, 0).unwrap();
        let q = b.divide_trunc(a).unwrap();
        assert_eq!(q.val, 19469696969);
    }

    #[test]
    fn test_divide_round_up() {
        let a = from_u64(1056, 0).unwrap();
        let b = from_u64(2056, 0).unwrap();
        let q = b.divide_round_up(a).unwrap();
        assert_eq!(q.val, 19469696970);
    }

    #[test]
    fn test_trunc_round_up_to_decimals() {
        let a = from_u64(1056, 3).unwrap();
        assert_eq!(a.trunc_to_decimals( 1).unwrap().val, 10000000000);
        assert_eq!(a.round_up_to_decimals( 1).unwrap().val, 11000000000);

        let b = from_u64(1534, 2).unwrap();
        assert_eq!(b.trunc_to_decimals( 3).unwrap().val, 153400000000);
        assert_eq!(b.round_up_to_decimals( 1).unwrap().val, 154000000000);
    }

    #[test]
    fn test_sqrt() {
        // Perfect square.
        // sqrt_approx(11.0889) <= 3.33
        assert!(from_u64(110889, 4).unwrap().sqrt_approx().unwrap().val <= 33300000000);

        // Non perfect square (greater than 10 decimal places).
        // sqrt_approx(110.889) <= 10.5303846083
        assert!(from_u64(110889, 3).unwrap().sqrt_approx().unwrap().val <= 105303846083);
    }

    #[test]
    fn test_divide_exceed_max() {
        let a = from_u64(1, 10).unwrap();
        let b = max_fp();
        assert_eq!(b.divide_trunc(a).unwrap_err(), NumberError::SizeOverflow);
    }
}
