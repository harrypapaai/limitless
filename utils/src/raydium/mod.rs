use anchor_lang::{declare_program, solana_program};

pub mod validate;
pub mod fees;
pub mod math;
pub mod curves;
pub mod state;
pub mod pda;

// TODO: look up actual index.
pub const INDEX: u16 = 0;

declare_program!(raydium_cp_swap);

pub const AMM_CONFIG_SEED: &str = "amm_config";
pub const AUTH_SEED: &str = "vault_and_lp_mint_auth_seed";
pub const POOL_SEED: &str = "pool";
pub const POOL_LP_MINT_SEED: &str = "pool_lp_mint";
pub const POOL_VAULT_SEED: &str = "pool_vault";
pub const OBSERVATION_SEED: &str = "observation";

pub const Q32: u128 = (u32::MAX as u128) + 1; // 2^32

#[cfg(feature = "localnet")]
pub const ID: solana_program::pubkey::Pubkey = solana_program::pubkey!("CVHAgHmDhZRjscris8N18pgwf8YS5WGjNUaNvhS7TC9e");
#[cfg(feature = "devnet")]
pub const ID: solana_program::pubkey::Pubkey = solana_program::pubkey!("CPMDWBwJDtYax9qW7AyRuVC19Cc4L4Vcy4n2BHAbHkCW");
#[cfg(not(any(feature = "localnet", feature = "devnet")))]
pub const ID: solana_program::pubkey::Pubkey = solana_program::pubkey!("DTKh8wubM3BKX4kMhGX4rNpCgNAikUFs8JedctWXhZP7");

#[cfg(feature = "localnet")]
pub const CREATE_POOL_FEE_RECEIVER: solana_program::pubkey::Pubkey = solana_program::pubkey!("CajCjc1MWZq7WvCnud5YxhYFWjsKtinD4yCyKK69tk2N");
#[cfg(feature = "devnet")]
pub const CREATE_POOL_FEE_RECEIVER: solana_program::pubkey::Pubkey = solana_program::pubkey!("G11FKBRaAkHAKuLCgLM6K6NUc9rTjPAznRCjZifrTQe2");
#[cfg(not(any(feature = "localnet", feature = "devnet")))]
pub const CREATE_POOL_FEE_RECEIVER: solana_program::pubkey::Pubkey = solana_program::pubkey!("3Rv8nNUAFeQmeJkxNMsuJYx1z4hG71z9sjZX3gbBLih1");
