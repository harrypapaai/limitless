use solana_program::pubkey::Pubkey;

pub const EVENT_AUTHORITY_SEED: &str = "__event_authority";
pub trait ToEventData {
    fn data(&self) -> Vec<u8>;
}

pub struct EventAuthoritySignerSeeds {
    pub bump: [u8; 1],
}

impl EventAuthoritySignerSeeds {
    pub fn new(program_id: &Pubkey) -> Self {
        let (_, bump) = Pubkey::find_program_address(
            &[
                EVENT_AUTHORITY_SEED.as_bytes(),
            ],
            program_id,
        );
        Self {
            bump: [bump],
        }
    }

    pub fn as_refs(&self) -> [&[u8]; 2] {
        [
            EVENT_AUTHORITY_SEED.as_bytes(),
            &self.bump,
        ]
    }
}
