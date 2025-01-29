use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;
use utils::state::Packable;
use blackwing_proc_macros::Packable;
use crate::errors::LimitlessError;

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq)]
#[borsh(use_discriminant=true)]
pub enum TradingMode {
    Disabled = 0,
    Enabled,
    PositionCloseOnly,
}

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Packable, Debug, PartialEq)]
pub struct ConfigAccount {
    pub trading_mode: TradingMode,
    pub fee_collector: Pubkey,
    pub space: [u8; 128],
}

impl ConfigAccount {
    pub fn signer_seeds() -> Result<ConfigAccountSignerSeeds, LimitlessError> {
        ConfigAccountSignerSeeds::new()
    }
}

pub struct ConfigAccountSignerSeeds {
    pub bump: [u8; 1],
}

impl ConfigAccountSignerSeeds {
    pub const SEED: &'static str = "config_account";

    pub fn new() -> Result<Self, LimitlessError> {
        let (_, bump) = Pubkey::find_program_address(
            &[Self::SEED.as_bytes()],
            &crate::ID,
        );
        Ok(Self {
            bump: [bump],
        })
    }

    pub fn as_refs(&self) -> [&[u8]; 2] {
        [
            Self::SEED.as_bytes(),
            &self.bump,
        ]
    }
}
