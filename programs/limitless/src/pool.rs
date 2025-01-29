use utils::numbers::SafeUnsigned;
use crate::errors::LimitlessError;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone)]
pub struct Pool {
    total_balance: u64,
    share_token_supply: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone)]
pub struct PoolPosition {
    share_token_amt: u64,
}

impl PoolPosition {
    pub fn new() -> Self {
        Self {
            share_token_amt: 0,
        }
    }

    pub fn zero(&mut self) {
        self.share_token_amt = 0;
    }

    pub fn share_token_amt(&self) -> u64 {
        self.share_token_amt
    }
}

impl Pool {
    const INITIAL_SHARE_TOKEN_AMT: u64 = 10_000;

    pub fn new() -> Self {
        Self {
            total_balance: 0,
            share_token_supply: 0,
        }
    }

    pub fn total_balance(&self) -> u64 {
        self.total_balance
    }

    pub fn share_token_supply(&self) -> u64 {
        self.share_token_supply
    }

    pub fn mint_position(&mut self, amt: u64) -> Result<PoolPosition, LimitlessError> {
        let mut pos = PoolPosition{
            share_token_amt: 0,
        };
        self.incr_position_amt(&mut pos, amt)?;
        Ok(pos)
    }

    pub fn decr_balance(&mut self, amt: u64) -> Result<(), LimitlessError> {
        self.total_balance = self.total_balance.safe_sub(amt)?;
        Ok(())
    }

    pub fn incr_balance(&mut self, amt: u64) -> Result<(), LimitlessError> {
        self.total_balance = self.total_balance.safe_add(amt)?;
        Ok(())
    }

    pub fn incr_position_amt(&mut self, pos: &mut PoolPosition, amt: u64) -> Result<(), LimitlessError> {
        let total_balance_u128 = self.total_balance as u128;
        let share_token_supply_u128 = self.share_token_supply as u128;
        let amt_u128 = amt as u128;

        let share_tokens_minted = if share_token_supply_u128 == 0 {
            Self::INITIAL_SHARE_TOKEN_AMT
        } else {
            share_token_supply_u128
                .safe_mul(amt_u128)?
                .safe_div(total_balance_u128)?
                .try_into()?
        };

        pos.share_token_amt = pos.share_token_amt.safe_add(share_tokens_minted)?;
        self.share_token_supply = self.share_token_supply.safe_add(share_tokens_minted)?;
        self.total_balance = self.total_balance.safe_add(amt)?;

        Ok(())
    }

    #[allow(dead_code)]
    fn decr_position_amt(&mut self, pos: &mut PoolPosition, amt: u64) -> Result<u64, LimitlessError> {
        let total_balance_u128 = self.total_balance as u128;
        let share_token_supply_u128 = self.share_token_supply as u128;
        let amt_u128 = amt as u128;

        let share_tokens_burned = share_token_supply_u128
            .safe_mul(amt_u128)?
            .safe_div(total_balance_u128)?
            .try_into()?;

        self.burn_position_shares(pos, share_tokens_burned)
    }

    pub fn burn_position_shares(&mut self, pos: &mut PoolPosition, amt: u64) -> Result<u64, LimitlessError> {
        let balance_redeemed = self.balance_redeemed_for_burn_position_shares(amt)?;

        pos.share_token_amt = pos.share_token_amt.safe_sub(amt)?;
        self.share_token_supply = self.share_token_supply.safe_sub(amt)?;
        self.total_balance = self.total_balance.safe_sub(balance_redeemed)?;

        Ok(balance_redeemed)
    }

    pub fn balance_redeemed_for_burn_position_shares(&self, amt: u64) -> Result<u64, LimitlessError> {
        let balance_redeemed = (self.total_balance as u128)
            .safe_mul(amt as u128)?
            .safe_div(self.share_token_supply as u128)?
            .try_into()?;
        Ok(balance_redeemed)
    }

    pub fn shares_needed_to_redeem_balance(&self, amt: u64) -> Result<u64, LimitlessError> {
        let shares_needed = (self.share_token_supply as u128)
            .safe_mul(amt as u128)?
            .safe_ceil_div(self.total_balance as u128)?
            .try_into()?;
        Ok(shares_needed)
    }

    pub fn burn_entire_position(&mut self, position: &mut PoolPosition) -> Result<u64, LimitlessError> {
        self.burn_position_shares(position, position.share_token_amt)
    }
}

// Helper to calculate ownership of increasing/decreasing balances according to percent
// ownership (separate from the balances being tracked).
//
// Increases in balance are only accrued to owners at the time of the increase.
//
// Used to track fees for LPs.

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone)]
pub struct BalancePool {
    total_balance: u64,
    total_fake_balance: u64,
    share_token_supply: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, PartialEq, Clone)]
pub struct BalancePoolPosition {
    share_token_amt: u64,
    fake_balance_created: u64,
}

impl BalancePoolPosition {
    pub fn new() -> Self {
        Self {
            share_token_amt: 0,
            fake_balance_created: 0,
        }
    }

    pub fn zero(&mut self) {
        self.share_token_amt = 0;
        self.fake_balance_created = 0;
    }

    pub fn fake_and_real_share_token_amt(&self) -> Result<u64, LimitlessError> {
        Ok(self.share_token_amt.safe_add(self.fake_balance_created)?)
    }

    pub fn share_token_amt(&self) -> u64 {
        self.share_token_amt
    }

    pub fn fake_balance(&self) -> u64 {
        self.fake_balance_created
    }
}

impl BalancePool {
    const INITIAL_SHARE_TOKEN_AMT: u64 = 10_000;

    pub fn new() -> Self {
        Self {
            total_balance: 0,
            total_fake_balance: 0,
            share_token_supply: 0,
        }
    }

    pub fn total_balance(&self) -> u64 {
        self.total_balance
    }

    pub fn fake_total_balance(&self) -> u64 {
        self.total_fake_balance
    }

    pub fn share_token_supply(&self) -> u64 {
        self.share_token_supply
    }

    pub fn incr_balance(&mut self, amt: u64) -> Result<(), LimitlessError> {
        self.total_balance = self.total_balance.safe_add(amt)?;
        Ok(())
    }

    pub fn decr_balance(&mut self, amt: u64) -> Result<(), LimitlessError> {
        self.total_balance = self.total_balance.safe_sub(amt)?;
        Ok(())
    }

    pub fn incr_position_share(
        &mut self,
        pos: &mut BalancePoolPosition,
        new_percent_ownership_num: u64,
        new_percent_ownership_den: u64,
    ) -> Result<(), LimitlessError> {

        let total_balance_u128 = self.total_balance as u128;
        let total_fake_balance_u128 = self.total_fake_balance as u128;
        let new_percent_liquidity_provided_num_u128 = new_percent_ownership_num as u128;
        let new_percent_liquidity_provided_den_u128 = new_percent_ownership_den as u128;
        let share_token_supply_u128 = self.share_token_supply as u128;

        let fake_balance_created = total_balance_u128.safe_add(total_fake_balance_u128)?
            .safe_mul(new_percent_liquidity_provided_num_u128)?
            .safe_div(new_percent_liquidity_provided_den_u128)?
            .try_into()?;
        let share_token_amt = if share_token_supply_u128 == 0 {
            Self::INITIAL_SHARE_TOKEN_AMT
        } else {
            share_token_supply_u128
                .safe_mul(new_percent_liquidity_provided_num_u128)?
                .safe_div(new_percent_liquidity_provided_den_u128)?
                .try_into()?
        };

        pos.fake_balance_created = pos.fake_balance_created.safe_add(fake_balance_created)?;
        pos.share_token_amt = pos.share_token_amt.safe_add(share_token_amt)?;

        self.share_token_supply = self.share_token_supply.safe_add(share_token_amt)?;
        self.total_fake_balance = self.total_fake_balance.safe_add(fake_balance_created)?;

        Ok(())
    }

    pub fn mint_position(&mut self, percent_ownership_num: u64, percent_ownership_den: u64) -> Result<BalancePoolPosition, LimitlessError> {
        let mut pos = BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        };
        self.incr_position_share(&mut pos, percent_ownership_num, percent_ownership_den)?;
        Ok(pos)
    }

    pub fn redeem_position(&mut self, position: &mut BalancePoolPosition) -> Result<u64, LimitlessError> {
        let balance_redeemed = self.total_balance.safe_add(self.total_fake_balance)?
            .safe_mul(position.share_token_amt)?
            .safe_div(self.share_token_supply)?
            .safe_sub(position.fake_balance_created)?;
        self.total_fake_balance = self.total_fake_balance.safe_sub(position.fake_balance_created)?;
        self.total_balance = self.total_balance.safe_sub(balance_redeemed)?;
        self.share_token_supply = self.share_token_supply.safe_sub(position.share_token_amt)?;

        position.zero();

        Ok(balance_redeemed)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pool_calcs_increase_decrease_balance() {
        let mut pool = Pool::new();

        // Initial deposit by user 1.
        let mut user_1_pos = pool.mint_position(100).unwrap();
        assert_eq!(user_1_pos.share_token_amt(), BalancePool::INITIAL_SHARE_TOKEN_AMT);

        // Fee balance increases.
        pool.incr_balance(100).unwrap();

        // User 2 deposits.
        let mut user_2_pos = pool.mint_position(200).unwrap();
        assert_eq!(user_2_pos.share_token_amt(), BalancePool::INITIAL_SHARE_TOKEN_AMT);

        // Another fee balance increase.
        pool.incr_balance(500).unwrap();
        pool.decr_balance(100).unwrap();

        // User 1 redeems.
        let received = pool.decr_position_amt(&mut user_1_pos, 20).unwrap();
        assert_eq!(received, 20);
        let received = pool.burn_entire_position(&mut user_1_pos).unwrap();
        assert_eq!(received, 380);
        assert_eq!(user_1_pos.share_token_amt(), 0);

        // Another fee balance change.
        pool.decr_balance(50).unwrap();

        // User 2 redeems.
        let received = pool.decr_position_amt(&mut user_2_pos, 30).unwrap();
        assert_eq!(received, 29); // rounding
        let received = pool.burn_entire_position(&mut user_2_pos).unwrap();
        assert_eq!(received, 321);

        assert_eq!(pool.total_balance, 0);
        assert_eq!(pool.share_token_supply, 0);
    }

    #[test]
    fn pool_calcs_user_2_sees_only_decrease_balance() {
        let mut pool = Pool::new();

        // Initial deposit by user 1.
        let mut user_1_pos = pool.mint_position(100).unwrap();
        assert_eq!(user_1_pos.share_token_amt(), BalancePool::INITIAL_SHARE_TOKEN_AMT);

        // Fee balance increases.
        pool.incr_balance(100).unwrap();

        // User 2 deposits.
        let mut user_2_pos = pool.mint_position(200).unwrap();
        assert_eq!(user_2_pos.share_token_amt(), BalancePool::INITIAL_SHARE_TOKEN_AMT);

        // Another fee balance increase.
        pool.decr_balance(50).unwrap();

        // User 1 redeems.
        let received = pool.burn_entire_position(&mut user_1_pos).unwrap();
        assert_eq!(received, 175);
        assert_eq!(user_1_pos.share_token_amt(), 0);

        // User 2 redeems.
        let received = pool.burn_entire_position(&mut user_2_pos).unwrap();
        assert_eq!(received, 175);
        assert_eq!(user_2_pos.share_token_amt(), 0);

        assert_eq!(pool.total_balance, 0);
        assert_eq!(pool.share_token_supply, 0);
    }

    #[test]
    fn pool_calcs_3_users() {
        let mut pool = Pool::new();

        // Initial deposit by user 1.
        let mut user_1_pos = pool.mint_position(100).unwrap();
        assert_eq!(user_1_pos.share_token_amt(), BalancePool::INITIAL_SHARE_TOKEN_AMT);

        // Fee balance increases.
        pool.incr_balance(100).unwrap();

        // User 2 deposits.
        let mut user_2_pos = pool.mint_position(200).unwrap();
        assert_eq!(user_2_pos.share_token_amt(), BalancePool::INITIAL_SHARE_TOKEN_AMT);

        // Another fee balance increase.
        pool.incr_balance(400).unwrap();

        // User 3 deposits.
        let mut user_3_pos = pool.mint_position(266).unwrap();
        assert_eq!(user_3_pos.share_token_amt(), 6650);

        // Another fee balance increase.
        pool.decr_balance(100).unwrap();
        pool.incr_balance(400).unwrap();

        // User 1 redeems.
        let received = pool.burn_entire_position(&mut user_1_pos).unwrap();
        assert_eq!(received, 512);

        // Another fee balance increase.
        pool.incr_balance(200).unwrap();

        // User 2 redeems.
        let received = pool.burn_entire_position(&mut user_2_pos).unwrap();
        assert_eq!(received, 633);

        // User 3 redeems.
        let received = pool.burn_entire_position(&mut user_3_pos).unwrap();
        assert_eq!(received, 421);

        assert_eq!(pool.total_balance, 0);
        assert_eq!(pool.share_token_supply, 0);
    }


    #[test]
    fn balance_pool_calcs() {
        let mut pool = BalancePool::new();

        // Initial deposit by user 1.
        let mut user_1_pos = pool.mint_position(1, 1).unwrap();
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Fee balance increases.
        pool.incr_balance(100).unwrap();

        // User 2 deposits.
        let mut user_2_pos = pool.mint_position(1, 1).unwrap();
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 100,
            share_token_amt: BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Another fee balance increase.
        pool.incr_balance(400).unwrap();

        // User 1 redeems.
        let fees_received = pool.redeem_position(&mut user_1_pos).unwrap();
        assert_eq!(fees_received, 300);
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        // Another fee balance increase.
        pool.incr_balance(200).unwrap();

        // User 2 redeems.
        let fees_received = pool.redeem_position(&mut user_2_pos).unwrap();
        assert_eq!(fees_received, 400);
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        assert_eq!(pool.total_balance, 0);
        assert_eq!(pool.share_token_supply, 0);
        assert_eq!(pool.total_fake_balance, 0);
    }


    #[test]
    fn balance_pool_calcs_3_users() {
        let mut pool = BalancePool::new();

        // Initial deposit by user 1.
        let mut user_1_pos = pool.mint_position(1, 1).unwrap();
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Fee balance increases.
        pool.incr_balance(100).unwrap();

        // User 2 deposits.
        let mut user_2_pos = pool.mint_position(1, 1).unwrap();
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 100,
            share_token_amt: BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Another fee balance increase.
        pool.incr_balance(400).unwrap();

        // User 3 deposits.
        let mut user_3_pos = pool.mint_position(333, 1000).unwrap();
        assert_eq!(user_3_pos, BalancePoolPosition {
            fake_balance_created: 199,
            share_token_amt: 6660,
        });

        // Another fee balance increase.
        pool.incr_balance(300).unwrap();

        // User 1 redeems.
        let fees_received = pool.redeem_position(&mut user_1_pos).unwrap();
        assert_eq!(fees_received, 412);
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        // Another fee balance increase.
        pool.incr_balance(200).unwrap();

        // User 2 redeems.
        let fees_received = pool.redeem_position(&mut user_2_pos).unwrap();
        assert_eq!(fees_received, 432);
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        // User 3 redeems.
        let fees_received = pool.redeem_position(&mut user_3_pos).unwrap();
        assert_eq!(fees_received, 156);
        assert_eq!(user_3_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        assert_eq!(pool.total_balance, 0);
        assert_eq!(pool.share_token_supply, 0);
        assert_eq!(pool.total_fake_balance, 0);
    }

    #[test]
    fn balance_pool_calcs_increase_pos_amt() {
        let mut pool = BalancePool::new();

        // Initial deposit by user 1.
        let mut user_1_pos = pool.mint_position(1, 1).unwrap();
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Fee balance increases.
        pool.incr_balance(100).unwrap();

        // User 2 deposits.
        let mut user_2_pos = pool.mint_position(2, 1).unwrap();
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 200,
            share_token_amt: 2*BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // User 1 increases position amt.
        pool.incr_position_share(&mut user_1_pos, 1, 3).unwrap();
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 100,
            share_token_amt: 2*BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Another fee balance increase.
        pool.incr_balance(400).unwrap();

        // User 3 deposits.
        let mut user_3_pos = pool.mint_position(333, 1000).unwrap();
        assert_eq!(user_3_pos, BalancePoolPosition {
            fake_balance_created: 266,
            share_token_amt: 13320,
        });

        // Another fee balance increase.
        pool.incr_balance(300).unwrap();

        // User 1 redeems.
        let fees_received = pool.redeem_position(&mut user_1_pos).unwrap();
        assert_eq!(fees_received, 412);
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        // Another fee balance increase.
        pool.incr_balance(200).unwrap();

        // User 2 redeems.
        let fees_received = pool.redeem_position(&mut user_2_pos).unwrap();
        assert_eq!(fees_received, 432);
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        // User 3 redeems.
        let fees_received = pool.redeem_position(&mut user_3_pos).unwrap();
        assert_eq!(fees_received, 156);
        assert_eq!(user_3_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        assert_eq!(pool.total_balance, 0);
        assert_eq!(pool.share_token_supply, 0);
        assert_eq!(pool.total_fake_balance, 0);
    }

    #[test]
    fn balance_pool_calcs_decrease_balance() {
        let mut pool = BalancePool::new();

        // Initial deposit by user 1.
        let mut user_1_pos = pool.mint_position(1, 1).unwrap();
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Fee balance decreases.
        pool.incr_balance(100).unwrap();

        // User 2 deposits.
        let mut user_2_pos = pool.mint_position(2, 1).unwrap();
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 200,
            share_token_amt: 2*BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // User 1 increases position amt.
        pool.incr_position_share(&mut user_1_pos, 1, 3).unwrap();
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 100,
            share_token_amt: 2*BalancePool::INITIAL_SHARE_TOKEN_AMT,
        });

        // Another fee balance increase.
        pool.incr_balance(400).unwrap();

        // User 3 deposits.
        let mut user_3_pos = pool.mint_position(333, 1000).unwrap();
        assert_eq!(user_3_pos, BalancePoolPosition {
            fake_balance_created: 266,
            share_token_amt: 13320,
        });

        // Another fee balance increase.
        pool.incr_balance(300).unwrap();

        // User 1 redeems.
        let fees_received = pool.redeem_position(&mut user_1_pos).unwrap();
        assert_eq!(fees_received, 412);
        assert_eq!(user_1_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        // Another fee balance increase.
        pool.incr_balance(200).unwrap();

        // User 2 redeems.
        let fees_received = pool.redeem_position(&mut user_2_pos).unwrap();
        assert_eq!(fees_received, 432);
        assert_eq!(user_2_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        // User 3 redeems.
        let fees_received = pool.redeem_position(&mut user_3_pos).unwrap();
        assert_eq!(fees_received, 156);
        assert_eq!(user_3_pos, BalancePoolPosition {
            fake_balance_created: 0,
            share_token_amt: 0,
        });

        assert_eq!(pool.total_balance, 0);
        assert_eq!(pool.share_token_supply, 0);
        assert_eq!(pool.total_fake_balance, 0);
    }
}