use std::ops::Deref;
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;
use crate::errors::LimitlessError;
use blackwing_proc_macros::Packable;
use utils::state::Packable;
use crate::pool::{BalancePool, Pool};
use crate::state::config::TradingMode;
use crate::state::is_data_cleared;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq)]
#[borsh(use_discriminant=true)]
pub enum QuoteToken {
    Token0 = 1,
    Token1,
}

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Packable, Debug, PartialEq)]
pub struct MarketAccount {
    // The trading mode of this market.
    pub trading_mode: TradingMode,
    // If token0 or token1 is the quote token. The other token will be the base token.
    pub quote_token: QuoteToken,
    // Base blackwing fee APR. Actual fee paid is a function of this and the utilization rate
    // of the available liquidity.
    pub base_fee_apr: u64,
    // The minimum fee in the quote token that will be charged for a position.
    pub min_fee_quote_token: u64,
    // The minimum duration in slots that a position can be opened for.
    pub min_duration_slots: u64,
    // The maximum duration in slots that a position can be opened for.
    pub max_duration_slots: u64,

    // The Pool representing the supplied LP tokens to the market.
    pub lp_tokens_supplied_pool: Pool,
    // The number of LP tokens removed from the market account for active positions.
    pub lp_tokens_removed_for_positions: u64,
    // The BalancePool representing fees accumulated as LP tokens.
    pub lp_token_fee_balance_pool: BalancePool,
    // The BalancePool representing fees accumulated as token_0.
    pub token_0_fee_balance_pool: BalancePool,
    // The BalancePool representing fees accumulated as token_1.
    pub token_1_fee_balance_pool: BalancePool,

    // The creator of the market.
    pub creator: Pubkey,

    // Extra account space.
    pub space: [u8; 128],
}

impl MarketAccount {
    #[inline]
    pub fn signer_seeds(
        token_0_mint: &Pubkey,
        token_1_mint: &Pubkey,
    ) -> Result<MarketAccountSignerSeeds, LimitlessError> {
        MarketAccountSignerSeeds::new(token_0_mint, token_1_mint)
    }

    #[inline]
    pub fn token_account_signer_seeds(
        market_account: &Pubkey,
        token_program: &Pubkey,
        token_mint: &Pubkey,
    ) -> MarketTokenAccountSignerSeeds {
        MarketTokenAccountSignerSeeds::new(market_account, token_program, token_mint)
    }

    #[inline]
    pub fn intermediate_token_account_signer_seeds(
        market_account: &Pubkey,
        token_program: &Pubkey,
        token_mint: &Pubkey,
    ) -> MarketIntermediateTokenAccountSignerSeeds {
        MarketIntermediateTokenAccountSignerSeeds::new(market_account, token_program, token_mint)
    }

    pub fn get_base_and_quote_keys<'a>(
        &self,
        token_0_mint: &'a Pubkey,
        token_1_mint: &'a Pubkey,
    ) -> (&'a Pubkey, &'a Pubkey) {

        match self.quote_token {
            QuoteToken::Token0 => (token_1_mint, token_0_mint),
            QuoteToken::Token1 => (token_0_mint, token_1_mint),
        }
    }
}

pub struct MarketAccountSignerSeeds {
    pub token_0_mint: [u8; 32],
    pub token_1_mint: [u8; 32],
    pub bump: [u8; 1],
}

impl MarketAccountSignerSeeds {
    pub const SEED: &'static str = "market_account";

    pub fn new(
        token_0_mint: &Pubkey,
        token_1_mint: &Pubkey,
    ) -> Result<Self, LimitlessError> {
        if token_1_mint <= token_0_mint {
            return Err(LimitlessError::InvalidTokenPairOrder);
        };
        let (_, bump) = Pubkey::find_program_address(
            &[
                Self::SEED.as_bytes(),
                token_0_mint.to_bytes().as_slice(),
                token_1_mint.to_bytes().as_slice(),
            ],
            &crate::ID,
        );
        Ok(Self {
            token_0_mint: token_0_mint.to_bytes(),
            token_1_mint: token_1_mint.to_bytes(),
            bump: [bump],
        })
    }

    pub fn as_refs(&self) -> [&[u8]; 4] {
        [
            Self::SEED.as_bytes(),
            self.token_0_mint.as_slice(),
            self.token_1_mint.as_slice(),
            &self.bump,
        ]
    }
}
pub struct MarketIntermediateTokenAccountSignerSeeds {
    pub market_account: [u8; 32],
    pub token_program: [u8; 32],
    pub token_mint: [u8; 32],
    pub bump: [u8; 1],
}

impl MarketIntermediateTokenAccountSignerSeeds {
    pub const SEED: &'static str = "market_intermediate_ta";
    pub fn new(
        market_account: &Pubkey,
        token_program: &Pubkey,
        token_mint: &Pubkey,
    ) -> Self {
        let (_, bump) = Pubkey::find_program_address(
            &[
                Self::SEED.as_bytes(),
                market_account.to_bytes().as_slice(),
                token_program.to_bytes().as_slice(),
                token_mint.to_bytes().as_slice(),
            ],
            &crate::ID,
        );
        Self {
            market_account: market_account.to_bytes(),
            token_program: token_program.to_bytes(),
            token_mint: token_mint.to_bytes(),
            bump: [bump],
        }
    }

    pub fn as_refs(&self) -> [&[u8]; 5] {
        [
            Self::SEED.as_bytes(),
            self.market_account.as_slice(),
            self.token_program.as_slice(),
            self.token_mint.as_slice(),
            &self.bump,
        ]
    }
}

pub struct MarketTokenAccountSignerSeeds {
    pub market_account: [u8; 32],
    pub token_program: [u8; 32],
    pub token_mint: [u8; 32],
    pub bump: [u8; 1],
}

impl MarketTokenAccountSignerSeeds {
    pub fn new(
        market_account: &Pubkey,
        token_program: &Pubkey,
        token_mint: &Pubkey,
    ) -> Self {
        let (_, bump) = Pubkey::find_program_address(
            &[
                market_account.to_bytes().as_slice(),
                token_program.to_bytes().as_slice(),
                token_mint.to_bytes().as_slice(),
            ],
            &crate::ID,
        );
        Self {
            market_account: market_account.to_bytes(),
            token_program: token_program.to_bytes(),
            token_mint: token_mint.to_bytes(),
            bump: [bump],
        }
    }

    pub fn as_refs(&self) -> [&[u8]; 4] {
        [
            self.market_account.as_slice(),
            self.token_program.as_slice(),
            self.token_mint.as_slice(),
            &self.bump,
        ]
    }
}

pub struct MarketState {
    // If token0 or token1 is the quote token. The other token will be the base token.
    quote_token: QuoteToken,
    // The balance of token 0 in the market account's ata.
    token_0_balance: u64,
    // The balance of token 1 in the market account's ata.
    token_1_balance: u64,
    // The balance of the raydium lp token in the market account's ata.
    raydium_lp_balance: u64,
}

pub struct MarketStateAccounts<'refs, 'info> {
    pub market_account: &'refs AccountInfo<'info>,
    pub token_0_ata: &'refs AccountInfo<'info>,
    pub token_1_ata: &'refs AccountInfo<'info>,
    pub raydium_lp_ata: &'refs AccountInfo<'info>,
}

impl MarketState {
    pub fn base_token_balance(&self) -> u64 {
        match self.quote_token {
            QuoteToken::Token0 => self.token_1_balance,
            QuoteToken::Token1 => self.token_0_balance,
        }
    }

    pub fn quote_token_balance(&self) -> u64 {
        match self.quote_token {
            QuoteToken::Token0 => self.token_0_balance,
            QuoteToken::Token1 => self.token_1_balance,
        }
    }

    pub fn token_0_balance(&self) -> u64 {
        self.token_0_balance
    }

    pub fn token_1_balance(&self) -> u64 {
        self.token_1_balance
    }

    pub fn lp_token_balance(&self) -> u64 {
        self.raydium_lp_balance
    }

    pub fn load(accounts: MarketStateAccounts) -> Result<Self, LimitlessError> {
        if is_data_cleared(accounts.market_account.data.borrow().deref()) {
            return Err(LimitlessError::MarketDoesNotExist);
        }

        let token_0_balance = utils::token::amount_from_token_account_info(accounts.token_0_ata)?;
        let token_1_balance = utils::token::amount_from_token_account_info(accounts.token_1_ata)?;
        let raydium_lp_balance = utils::token::amount_from_token_account_info(accounts.raydium_lp_ata)?;

        let market_account = MarketAccount::unpack(accounts.market_account)
            .map_err(|_| LimitlessError::MarketStateSerializationFailed)?;

        Ok(Self {
            quote_token: market_account.quote_token,
            token_0_balance,
            token_1_balance,
            raydium_lp_balance,
        })
    }

    pub fn reload(&mut self, accounts: MarketStateAccounts) -> Result<(), LimitlessError> {
        self.token_0_balance = utils::token::amount_from_token_account_info(accounts.token_0_ata)?;
        self.token_1_balance = utils::token::amount_from_token_account_info(accounts.token_1_ata)?;
        self.raydium_lp_balance = utils::token::amount_from_token_account_info(accounts.raydium_lp_ata)?;
        Ok(())
    }
}
