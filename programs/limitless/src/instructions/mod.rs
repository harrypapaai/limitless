pub mod open_position;
pub mod init_market;
pub mod admin_init_limitless;
pub mod close_position;
pub mod deposit_liquidity;
pub mod withdraw_liquidity;
pub mod test;
pub mod rollover_position;
pub mod edit_position_rollover;
pub mod update_market_config;

mod utils;

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::account_info::AccountInfo;
use blackwing_proc_macros::instruction_args;
use crate::errors::{LimitlessError};
use crate::instructions::deposit_liquidity::MintArgs;
use crate::state::config::TradingMode;
use crate::state::market::QuoteToken;

#[cfg(feature = "localnet")]
#[repr(C)]
#[instruction_args]
#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq)]
pub enum LimitlessInstruction {
    AdminInit,
    InitMarket{
        trading_mode: TradingMode,
        quote_token: QuoteToken,
        base_fee_apr: u64,
        min_fee_quote_token: u64,
        min_duration_slots: u64,
        max_duration_slots: u64,
    },
    UpdateMarketConfig{
        trading_mode: Option<TradingMode>,
        base_fee_apr: Option<u64>,
        min_fee_quote_token: Option<u64>,
        min_duration_slots: Option<u64>,
        max_duration_slots: Option<u64>,
    },
    OpenPosition{
        id: uuid::Uuid,
        collateral_quote_token_amt: u64,
        is_short: bool,
        duration_blocks: u64,
        max_raydium_fee_quote_token_amt: u64,
        max_blackwing_fee_quote_token_amt: u64,
        rollover_reserve_quote_token_amt: u64,
        worst_price_num: u64,
        worst_price_den: u64,
    },
    ClosePosition{
        id: uuid::Uuid,
        worst_price_num: u64,
        worst_price_den: u64,
    },
    RolloverPosition{
        id: uuid::Uuid,
        max_fee_override_quote_token_amt: Option<u64>,
        rollover_duration_override: Option<u64>,
    },
    EditPositionRollover{
        id: uuid::Uuid,
        max_fee_quote_token_amt: Option<u64>,
        rollover_duration: Option<u64>,
        new_rollover_reserve_quote_token_amt: Option<u64>,
    },
    DepositLpTokens{
        lp_token_amt: u64,
    },
    MintLpTokensAndDeposit{
        lp_token_amt: u64,
        max_token_0_amt: u64,
        max_token_1_amt: u64,
    },
    WithdrawLpTokens{
        share_amt: u64,
        min_received_lp_tokens: u64,
        burn_max: bool,
    },
    WithdrawAllLpTokens{
        min_received_lp_tokens: u64,
        burn_max: bool,
    },
    Cpi{
        data: Vec<u8>
    },
    MarketToDestTokenTransfer{
        amt: u64,
    },
}

#[cfg(not(feature = "localnet"))]
#[repr(C)]
#[instruction_args]
#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq)]
pub enum LimitlessInstruction {
    AdminInit,
    InitMarket{
        trading_mode: TradingMode,
        quote_token: QuoteToken,
        base_fee_apr: u64,
        min_fee_quote_token: u64,
        min_duration_slots: u64,
        max_duration_slots: u64,
    },
    UpdateMarketConfig{
        trading_mode: Option<TradingMode>,
        base_fee_apr: Option<u64>,
        min_fee_quote_token: Option<u64>,
        min_duration_slots: Option<u64>,
        max_duration_slots: Option<u64>,
    },
    OpenPosition{
        id: uuid::Uuid,
        collateral_quote_token_amt: u64,
        is_short: bool,
        duration_blocks: u64,
        max_raydium_fee_quote_token_amt: u64,
        max_blackwing_fee_quote_token_amt: u64,
        rollover_reserve_quote_token_amt: u64,
        worst_price_num: u64,
        worst_price_den: u64,
    },
    ClosePosition{
        id: uuid::Uuid,
        worst_price_num: u64,
        worst_price_den: u64,
    },
    RolloverPosition{
        id: uuid::Uuid,
        max_fee_override_quote_token_amt: Option<u64>,
        rollover_duration_override: Option<u64>,
    },
    EditPositionRollover{
        id: uuid::Uuid,
        max_fee_quote_token_amt: Option<u64>,
        rollover_duration: Option<u64>,
        new_rollover_reserve_quote_token_amt: Option<u64>,
    },
    DepositLpTokens{
        lp_token_amt: u64,
    },
    MintLpTokensAndDeposit{
        lp_token_amt: u64,
        max_token_0_amt: u64,
        max_token_1_amt: u64,
    },
    WithdrawLpTokens{
        share_amt: u64,
        min_received_lp_tokens: u64,
        burn_max: bool,
    },
    WithdrawAllLpTokens{
        min_received_lp_tokens: u64,
        burn_max: bool,
    },
    Cpi{
        data: Vec<u8>
    },
}

impl LimitlessInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, LimitlessError> {
        BorshDeserialize::try_from_slice(input).map_err(|_| LimitlessError::InvalidInstruction)
    }

    pub fn process(self, accounts: &[AccountInfo]) -> Result<(), LimitlessError> {
        match self {
            LimitlessInstruction::AdminInit => {
                admin_init_limitless::process_init_admin(accounts)
            }
            LimitlessInstruction::InitMarket{
                trading_mode,
                quote_token,
                base_fee_apr,
                min_fee_quote_token,
                min_duration_slots,
                max_duration_slots,
            } => {
                init_market::process_init_market(
                    accounts,
                    trading_mode,
                    quote_token,
                    base_fee_apr,
                    min_fee_quote_token,
                    min_duration_slots,
                    max_duration_slots,
                )
            }
            LimitlessInstruction::UpdateMarketConfig {
                trading_mode,
                base_fee_apr,
                min_fee_quote_token,
                min_duration_slots,
                max_duration_slots,
            } => {
                update_market_config::process_update_market_config(
                    accounts,
                    trading_mode,
                    base_fee_apr,
                    min_fee_quote_token,
                    min_duration_slots,
                    max_duration_slots,
                )
            }
            LimitlessInstruction::OpenPosition {
                id,
                collateral_quote_token_amt,
                is_short,
                duration_blocks,
                max_raydium_fee_quote_token_amt,
                max_blackwing_fee_quote_token_amt,
                rollover_reserve_quote_token_amt,
                worst_price_num,
                worst_price_den,
            } => {
                open_position::process_open_position(
                    accounts,
                    id,
                    collateral_quote_token_amt,
                    is_short,
                    duration_blocks,
                    max_raydium_fee_quote_token_amt,
                    max_blackwing_fee_quote_token_amt,
                    rollover_reserve_quote_token_amt,
                    worst_price_num,
                    worst_price_den,
                )
            }
            LimitlessInstruction::ClosePosition {
                id,
                worst_price_num,
                worst_price_den,
            } => {
                close_position::process_close_position(
                    accounts,
                    id,
                    worst_price_num,
                    worst_price_den,
                )
            }
            LimitlessInstruction::RolloverPosition {
                id,
                max_fee_override_quote_token_amt,
                rollover_duration_override,
            } => {
                rollover_position::process_rollover_position(
                    accounts,
                    id,
                    max_fee_override_quote_token_amt,
                    rollover_duration_override,
                )
            }
            LimitlessInstruction::EditPositionRollover {
                id,
                max_fee_quote_token_amt,
                rollover_duration,
                new_rollover_reserve_quote_token_amt,
            } => {
                edit_position_rollover::process_edit_position_rollover(
                    accounts,
                    id,
                    max_fee_quote_token_amt,
                    rollover_duration,
                    new_rollover_reserve_quote_token_amt,
                )
            }
            LimitlessInstruction::DepositLpTokens {
                lp_token_amt,
            } => {
                deposit_liquidity::process_deposit_liquidity(accounts, lp_token_amt, None)
            }
            LimitlessInstruction::MintLpTokensAndDeposit {
                lp_token_amt,
                max_token_0_amt,
                max_token_1_amt,
            } => {
                deposit_liquidity::process_deposit_liquidity(accounts, lp_token_amt, Some(MintArgs{
                    max_token_0_amt,
                    max_token_1_amt,
                }))
            }
            LimitlessInstruction::WithdrawLpTokens{
                share_amt,
                min_received_lp_tokens,
                burn_max,
            } => {
                withdraw_liquidity::process_withdraw_liquidity(
                    accounts,
                    withdraw_liquidity::BurnArgs::PartialPosition {
                        share_amt,
                        burn_max
                    },
                    min_received_lp_tokens,
                )
            }
            LimitlessInstruction::WithdrawAllLpTokens{
                min_received_lp_tokens,
                burn_max,
            } => {
                withdraw_liquidity::process_withdraw_liquidity(
                    accounts,
                    withdraw_liquidity::BurnArgs::EntirePosition { burn_max },
                    min_received_lp_tokens,
                )
            }
            LimitlessInstruction::Cpi {
                data: _data,
            } => {
                Ok(())
            }
            #[cfg(feature = "localnet")]
            LimitlessInstruction::MarketToDestTokenTransfer{
                amt,
            } => {
                test::transfer::market_to_dest_transfer_for_test(accounts, amt)
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn test_instruction_unpack() {
//         let instruction = crate::instructions::LimitlessInstruction::OpenPosition {
//             id: uuid::Uuid::new_v4(),
//             collateral_token_1_amt: 100,
//             is_short: false,
//             duration_blocks: 100,
//             max_blackwing_fee_token_1_amt: 10,
//             worst_price_fp64: 100,
//         };
//         let packed = borsh::to_vec(&instruction).unwrap();
//         let unpacked = crate::instructions::LimitlessInstruction::unpack(&packed).unwrap();
//         assert_eq!(instruction, unpacked);
//
//         let instruction = crate::instructions::LimitlessInstruction::ClosePosition {
//             id: uuid::Uuid::new_v4(),
//         };
//         let packed = borsh::to_vec(&instruction).unwrap();
//         let unpacked = crate::instructions::LimitlessInstruction::unpack(&packed).unwrap();
//         assert_eq!(instruction, unpacked);
//     }
// }

