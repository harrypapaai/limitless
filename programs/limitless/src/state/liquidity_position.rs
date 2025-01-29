use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;
use blackwing_proc_macros::Packable;
use utils::state::Packable;
use crate::errors::LimitlessError;
use crate::pool::{BalancePoolPosition, PoolPosition};

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Packable, Debug, PartialEq)]
pub struct LiquidityPositionAccount {
    // Amount of liquidity provided.
    pub lp_token_pool_position: PoolPosition,
    // Position in balance pool for fees accrued in LP tokens.
    pub lp_token_fee_pool_position: BalancePoolPosition,
    // Position in balance pool for fees accrued in token_0.
    pub token_0_fee_pool_position: BalancePoolPosition,
    // Position in balance pool for fees accrued in token_1.
    pub token_1_fee_pool_position: BalancePoolPosition,
    // Extra account space.
    pub space: [u8; 128],
}

impl LiquidityPositionAccount {
    #[inline]
    pub fn signer_seeds(
        market_account: &Pubkey,
        user_account: &Pubkey,
    ) -> Result<LiquidityPositionAccountSignerSeeds, LimitlessError> {
        LiquidityPositionAccountSignerSeeds::new(market_account, user_account)
    }
}

pub struct LiquidityPositionAccountSignerSeeds {
    pub market_account: [u8; 32],
    pub user_account: [u8; 32],
    pub bump: [u8; 1],
}

impl LiquidityPositionAccountSignerSeeds {
    pub const SEED: &'static str = "liquidity_position_account";

    pub fn new(
        market_account: &Pubkey,
        user_account: &Pubkey,
    ) -> Result<Self, LimitlessError> {
        let (_, bump) = Pubkey::find_program_address(
            &[
                Self::SEED.as_bytes(),
                market_account.to_bytes().as_slice(),
                user_account.to_bytes().as_slice(),
            ],
            &crate::ID,
        );
        Ok(Self {
            market_account: market_account.to_bytes(),
            user_account: user_account.to_bytes(),
            bump: [bump],
        })
    }

    pub fn as_refs(&self) -> [&[u8]; 4] {
        [
            Self::SEED.as_bytes(),
            self.market_account.as_slice(),
            self.user_account.as_slice(),
            &self.bump,
        ]
    }
}