use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;
use utils::state::Packable;
use blackwing_proc_macros::Packable;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Packable, Debug, PartialEq)]
pub struct PositionAccount {
    pub id: uuid::Uuid,
    pub market_account: Pubkey,
    pub position_size: u64,
    pub user_quote_token_collateral_amt: u64,
    pub collateral_amt: u64,
    pub raydium_fee_reserve_collateral_token_amt: u64,
    pub rounding_fee_reserve_collateral_token_amt: u64,
    pub blackwing_fee_reserve_quote_token_amt: u64,
    pub base_token_balance: u64,
    pub quote_token_balance: u64,
    pub lp_tokens_removed: u64,
    pub loan_position_token_amt: u64,
    pub loan_collateral_token_amt: u64,
    pub is_short: bool,
    pub open_block: u64,
    pub close_block: u64, // Updated on rollover.
    // The price before doing performing any actions against the pool to open the position.
    pub open_base_token_amt: u64,
    pub open_quote_token_amt: u64,
    // The price after fully opening the position.
    pub after_open_base_token_amt: u64,
    pub after_open_quote_token_amt: u64,
    // Rollover settings.
    pub rollover_max_fee_quote_token_amt: u64,
    pub rollover_duration_blocks: u64,
    pub rollover_reserve_quote_token_amt: u64,

    // Extra account space.
    pub raydium_fees_reserved_amt_quote_token: u64,
    pub space: [u8; 120],
}

impl PositionAccount {
    #[inline]
    pub fn signer_seeds(
        market_account: &Pubkey,
        user_account: &Pubkey,
        id: &uuid::Uuid,
    ) -> PositionAccountSignerSeeds {
        PositionAccountSignerSeeds::new(market_account, user_account, id)
    }
}

pub struct PositionAccountSignerSeeds {
    pub market_account: [u8; 32],
    pub user: [u8; 32],
    pub id: [u8; 16],
    pub bump: [u8; 1],
}

impl PositionAccountSignerSeeds {
    pub const SEED: &'static str = "position_account";

    pub fn new(
        market_account: &Pubkey,
        user: &Pubkey,
        id: &uuid::Uuid,
    ) -> Self {
        let (_, bump) = Pubkey::find_program_address(
            &[
                Self::SEED.as_bytes(),
                market_account.to_bytes().as_slice(),
                user.to_bytes().as_slice(),
                id.to_bytes_le().as_slice(),
            ],
            &crate::ID,
        );
        Self {
            market_account: market_account.to_bytes(),
            user: user.to_bytes(),
            id: id.to_bytes_le(),
            bump: [bump],
        }
    }

    pub fn as_refs(&self) -> [&[u8]; 5] {
        [
            Self::SEED.as_bytes(),
            self.market_account.as_slice(),
            self.user.as_slice(),
            self.id.as_slice(),
            &self.bump,
        ]
    }
}
