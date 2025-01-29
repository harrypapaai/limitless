pub mod market;
pub mod position;
pub mod config;
pub mod liquidity_position;

pub fn is_data_cleared(data: &[u8]) -> bool {
    data.iter().all(|&x| x == 0)
}

pub fn clear_data(data: &mut [u8]) {
    data.fill(0);
}
