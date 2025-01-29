use solana_program::pubkey::Pubkey;
use blackwing_proc_macros::blackwing_event;
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::instruction::Instruction;
use utils::events::{ToEventData};
use crate::instructions::LimitlessInstruction::Cpi;
use crate::state::config::TradingMode;
use crate::state::market::QuoteToken;

#[blackwing_event]
pub struct InitMarketEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,
    pub raydium_config: Pubkey,
    pub raydium_pool_state: Pubkey,
    // Was added because originally, we were returning token0/token1 but we changed the event
    // to return base_token/quote_token
    pub quote_token: QuoteToken,
    pub min_duration: u64,
    pub max_duration: u64,
    pub min_fee: u64,
    pub base_fee_apr: u64,
    pub creator: Pubkey,
}

#[blackwing_event]
pub struct UpdateMarketConfigEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,
    pub trading_mode: TradingMode,
    pub min_duration: u64,
    pub max_duration: u64,
    pub min_fee: u64,
    pub base_fee_apr: u64,
}

#[blackwing_event]
pub struct MarketStateUpdateEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,

    pub account_base_token_balance: u64,
    pub account_quote_token_balance: u64,
    pub account_lp_token_balance: u64,
    pub lp_tokens_removed_for_positions: u64,

    pub lp_tokens_supplied_total_shares: u64,
    pub lp_tokens_supplied_total_balance: u64,
    pub base_token_fees_total_shares: u64,
    pub base_token_fees_total_balance: u64,
    pub base_token_fees_total_fake_balance: u64,
    pub quote_token_fees_total_shares: u64,
    pub quote_token_fees_total_balance: u64,
    pub quote_token_fees_total_fake_balance: u64,
    pub lp_token_fees_total_shares: u64,
    pub lp_token_fees_total_balance: u64,
    pub lp_token_fees_total_fake_balance: u64,
}

#[blackwing_event]
pub struct PositionOpenEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,
    pub user_address: Pubkey,
    pub id: uuid::Uuid,

    pub position_size: u64,
    pub user_collateral_amt: u64,
    pub collateral_amt: u64,
    pub is_short: bool,
    pub lp_tokens_removed: u64,
    pub loan_position_token_amt: u64,
    pub loan_collateral_token_amt: u64,
    pub open_block: u64,
    pub close_block: u64,
    pub open_x: u64,
    pub open_y: u64,
    pub after_open_x: u64,
    pub after_open_y: u64,

    pub blackwing_fee_reserve_amt: u64,
    pub rollover_max_fee_amt: u64,
    pub rollover_duration_blocks: u64,
    pub rollover_fee_reserve_amt: u64,

    pub position_token_balance: u64,
    pub collateral_token_balance: u64,

    pub raydium_fee_reserve_amt_quote_token: u64,
}

#[blackwing_event]
pub struct PositionCloseEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,
    pub user_address: Pubkey,
    pub id: uuid::Uuid,
    pub open_block: u64,

    pub blackwing_fees_charged: u64,
    pub amt_transferred_to_user: u64,

    pub close_x: u64,
    pub close_y: u64,
    pub after_close_x: u64,
    pub after_close_y: u64,

    pub raydium_fee_charged_amt_quote_token: u64,

}

#[blackwing_event]
pub struct DepositOrWithdrawLiquidityEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,
    pub user_address: Pubkey,

    pub is_withdraw: bool,
    pub lp_tokens_change: u64,
    pub new_lp_position_share_token_amt: u64,
    pub new_base_token_fee_share_amt: u64,
    pub new_base_token_fake_balance: u64,
    pub new_quote_token_fee_share_amt: u64,
    pub new_quote_token_fake_balance: u64,
    pub new_lp_token_fee_share_amt: u64,
    pub new_lp_token_fake_balance: u64,
}

#[blackwing_event]
pub struct EditPositionRolloverEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,
    pub user_address: Pubkey,
    pub id: uuid::Uuid,
    pub open_block: u64,

    pub rollover_max_fee_amt: u64,
    pub rollover_duration_blocks: u64,
    pub rollover_fee_reserve_amt: u64,
}

#[blackwing_event]
pub struct RolloverPositionEvent {
    pub base_token_mint: Pubkey,
    pub quote_token_mint: Pubkey,
    pub user_address: Pubkey,
    pub id: uuid::Uuid,
    pub open_block: u64,

    pub blackwing_fee_reserve_amt: u64,
    pub rollover_fee_reserve_amt: u64,
}

pub fn emit_cpi_ix(event: &impl ToEventData, event_authority: &Pubkey) -> Instruction {
    let inner_data = event.data();
    let cpi_ix = Cpi {
        data: inner_data
    };
    let mut cpi_ix_data =  Vec::new();
    cpi_ix.serialize(&mut cpi_ix_data).unwrap();
    Instruction::new_with_bytes(
        crate::ID,
        &cpi_ix_data,
        vec![
            solana_program::instruction::AccountMeta::new_readonly(
                *event_authority,
                true,
            ),
        ],
    )
}
