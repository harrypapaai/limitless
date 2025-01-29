use std::ops::BitAnd;
use crate::raydium::raydium_cp_swap::accounts::PoolState;

pub enum PoolStatusBitIndex {
    Deposit,
    Withdraw,
    Swap,
}

pub fn get_pool_status_by_bit(pool_state: &PoolState, bit: PoolStatusBitIndex) -> bool {
    let status = u8::from(1) << (bit as u8);
    pool_state.status.bitand(status) == 0
}
