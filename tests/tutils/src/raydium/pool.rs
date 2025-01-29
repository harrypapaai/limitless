use solana_program::pubkey::Pubkey;
use std::ops::{BitAnd, BitOr, BitXor};
use borsh::{BorshSerialize, BorshDeserialize};

pub const POOL_SEED: &str = "pool";
pub const POOL_LP_MINT_SEED: &str = "pool_lp_mint";
pub const POOL_VAULT_SEED: &str = "pool_vault";

pub enum PoolStatusBitIndex {
    Deposit,
    Withdraw,
    Swap,
}

#[derive(PartialEq, Eq)]
pub enum PoolStatusBitFlag {
    Enable,
    Disable,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct PoolState {
    /// Which config the pool belongs
    pub amm_config: Pubkey,
    /// pool creator
    pub pool_creator: Pubkey,
    /// Token A
    pub token_0_vault: Pubkey,
    /// Token B
    pub token_1_vault: Pubkey,

    /// Pool tokens are issued when A or B tokens are deposited.
    /// Pool tokens can be withdrawn back to the original A or B token.
    pub lp_mint: Pubkey,
    /// Mint information for token A
    pub token_0_mint: Pubkey,
    /// Mint information for token B
    pub token_1_mint: Pubkey,

    /// token_0 program
    pub token_0_program: Pubkey,
    /// token_1 program
    pub token_1_program: Pubkey,

    /// observation account to store oracle data
    pub observation_key: Pubkey,

    pub auth_bump: u8,
    /// Bitwise representation of the state of the pool
    /// bit0, 1: disable deposit(vaule is 1), 0: normal
    /// bit1, 1: disable withdraw(vaule is 2), 0: normal
    /// bit2, 1: disable swap(vaule is 4), 0: normal
    pub status: u8,

    pub lp_mint_decimals: u8,
    /// mint0 and mint1 decimals
    pub mint_0_decimals: u8,
    pub mint_1_decimals: u8,

    /// True circulating supply without burns and lock ups
    pub lp_supply: u64,
    /// The amounts of token_0 and token_1 that are owed to the liquidity provider.
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,

    /// The timestamp allowed for swap in the pool.
    pub open_time: u64,
    /// padding for future updates
    pub padding: [u64; 32],
}

impl PoolState {
    pub fn set_status(&mut self, status: u8) {
        self.status = status
    }

    pub fn set_status_by_bit(&mut self, bit: PoolStatusBitIndex, flag: PoolStatusBitFlag) {
        let s = u8::from(1) << (bit as u8);
        if flag == PoolStatusBitFlag::Disable {
            self.status = self.status.bitor(s);
        } else {
            let m = u8::from(255).bitxor(s);
            self.status = self.status.bitand(m);
        }
    }

    /// Get status by bit, if it is `noraml` status, return true
    pub fn get_status_by_bit(&self, bit: PoolStatusBitIndex) -> bool {
        let status = u8::from(1) << (bit as u8);
        self.status.bitand(status) == 0
    }

    pub fn vault_amount_without_fee(&self, vault_0: u64, vault_1: u64) -> (u64, u64) {
        (
            vault_0
                .checked_sub(self.protocol_fees_token_0 + self.fund_fees_token_0)
                .unwrap(),
            vault_1
                .checked_sub(self.protocol_fees_token_1 + self.fund_fees_token_1)
                .unwrap(),
        )
    }

    pub fn token_price_x32(&self, vault_0: u64, vault_1: u64) -> (u128, u128) {
        let (token_0_amount, token_1_amount) = self.vault_amount_without_fee(vault_0, vault_1);
        (
            token_1_amount as u128 * utils::raydium::Q32 as u128 / token_0_amount as u128,
            token_0_amount as u128 * utils::raydium::Q32 as u128 / token_1_amount as u128,
        )
    }
}