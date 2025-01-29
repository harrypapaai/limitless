use solana_program::account_info::AccountInfo;
use crate::errors::LimitlessError;
use utils::{self, raydium::raydium_cp_swap};
use crate::state::market::{MarketAccount, QuoteToken};

pub struct RaydiumStateAccounts<'refs, 'info> {
    pub amm_config: &'refs AccountInfo<'info>,
    pub pool_state: &'refs AccountInfo<'info>,
    pub token_0_vault: &'refs AccountInfo<'info>,
    pub token_1_vault: &'refs AccountInfo<'info>,
}

#[derive(Debug)]
pub struct RaydiumState {
    // Total lp supply of the pool, obtained from pool state account.
    lp_supply: u64,
    // Total amount of token 0 in the pool, obtained from token 0 vault account.
    // Does not include protocol and fund fees.
    token_0_amt: u64,
    // Total amount of token 0 in the pool but with fees included.
    token_0_amt_with_fees: u64,
    // Total amount of token 1 in the pool, obtained from token 1 vault account.
    // Does not include protocol and fund fees.
    token_1_amt: u64,
    // Total amount of token 1 in the pool but with fees included.
    token_1_amt_with_fees: u64,
    // Fee rate charged based on amount of input tokens to swaps.
    trade_fee_rate: u64,
    // Percentage of trade fee that goes to protocol.
    protocol_fee_rate: u64,
    // Percentage of trade fee that goes to fund.
    fund_fee_rate: u64,
}

impl RaydiumState {
    pub fn token_0_amt(&self) -> u64 {
        self.token_0_amt
    }

    pub fn token_1_amt(&self) -> u64 {
        self.token_1_amt
    }

    pub fn base_token_amt(&self, market: &MarketAccount) -> u64 {
        match market.quote_token {
            QuoteToken::Token0 => self.token_1_amt,
            QuoteToken::Token1 => self.token_0_amt,
        }
    }

    pub fn quote_token_amt(&self, market: &MarketAccount) -> u64 {
        match market.quote_token {
            QuoteToken::Token0 => self.token_0_amt,
            QuoteToken::Token1 => self.token_1_amt,
        }
    }

    pub fn lp_supply(&self) -> u64 {
        self.lp_supply
    }

    pub fn trade_fee_rate(&self) -> u64 {
        self.trade_fee_rate
    }

    pub fn protocol_fee_rate(&self) -> u64 {
        self.protocol_fee_rate
    }

    pub fn fund_fee_rate(&self) -> u64 {
        self.fund_fee_rate
    }

    pub fn load(accounts: RaydiumStateAccounts) -> Result<Self, LimitlessError> {
        let pool_state = {
            utils::state::anchor_unpack_info
                ::<raydium_cp_swap::accounts::PoolState>(accounts.pool_state)
                .map_err(|_| LimitlessError::UtilsErrorRaydiumPoolStateSerializationFailed)?
        };

        let amm_config = utils::state::anchor_unpack_info
                ::<raydium_cp_swap::accounts::AmmConfig>(accounts.amm_config)
            .map_err(|_| LimitlessError::UtilsErrorRaydiumAmmConfigSerializationFailed)?;

        let token_0_amt_with_fees = utils::token::amount_from_token_account_info(accounts.token_0_vault)?;
        let token_1_amt_with_fees = utils::token::amount_from_token_account_info(accounts.token_1_vault)?;
        let (token_0_amt, token_1_amt) = utils::raydium::fees::vault_amount_without_fee(
            &pool_state,
            token_0_amt_with_fees,
            token_1_amt_with_fees,
        )?;

        Ok(Self {
            lp_supply: pool_state.lp_supply,
            token_0_amt_with_fees,
            token_1_amt_with_fees,
            token_0_amt,
            token_1_amt,
            trade_fee_rate: amm_config.trade_fee_rate,
            protocol_fee_rate: amm_config.protocol_fee_rate,
            fund_fee_rate: amm_config.fund_fee_rate,
        })
    }

    pub fn reload(&mut self, accounts: RaydiumStateAccounts) -> Result<(), LimitlessError> {
        let pool_state = utils::state::anchor_unpack_info
            ::<raydium_cp_swap::accounts::PoolState>(accounts.pool_state)
            .map_err(|_| LimitlessError::UtilsErrorRaydiumPoolStateSerializationFailed)?;
        self.lp_supply = pool_state.lp_supply;
        self.token_0_amt_with_fees = utils::token::amount_from_token_account_info(accounts.token_0_vault)?;
        self.token_1_amt_with_fees = utils::token::amount_from_token_account_info(accounts.token_1_vault)?;
        let (token_0_amt, token_1_amt) = utils::raydium::fees::vault_amount_without_fee(
            &pool_state,
            self.token_0_amt_with_fees,
            self.token_1_amt_with_fees,
        )?;
        self.token_0_amt = token_0_amt;
        self.token_1_amt = token_1_amt;

        Ok(())
    }
}