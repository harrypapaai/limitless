use solana_program::pubkey::Pubkey;
use crate::errors::UtilsError;

pub fn amm_config_pda() -> Result<Pubkey, UtilsError> {
    let (amm_config_account, _) = Pubkey::try_find_program_address(
        &[
            &crate::raydium::AMM_CONFIG_SEED.as_bytes(),
            &crate::raydium::INDEX.to_be_bytes(), // index.
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?;
    Ok(amm_config_account)
}

pub fn token_vault_pda(
    vault_token: &Pubkey,
    token_0: &Pubkey,
    token_1: &Pubkey,
) -> Result<Pubkey, UtilsError> {
    let (token_vault_account, _) = Pubkey::try_find_program_address(
        &[
            &crate::raydium::POOL_VAULT_SEED.as_bytes(),
            &pool_state_pda(token_0, token_1)?.to_bytes(),
            &vault_token.to_bytes(),
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?;
    Ok(token_vault_account)
}

pub fn authority_pda() -> Result<Pubkey, UtilsError> {
    let (authority_account, _) = authority_pda_with_bump()?;
    Ok(authority_account)
}

pub fn authority_pda_with_bump() -> Result<(Pubkey, u8), UtilsError> {
    Ok(Pubkey::try_find_program_address(
        &[
            &crate::raydium::AUTH_SEED.as_bytes(),
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?)
}

pub fn observation_pda(token_0: &Pubkey, token_1: &Pubkey) -> Result<Pubkey, UtilsError> {
    let (observation_state_account, _) = Pubkey::try_find_program_address(
        &[
            &crate::raydium::OBSERVATION_SEED.as_bytes(),
            &pool_state_pda(token_0, token_1)?.to_bytes(),
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?;
    Ok(observation_state_account)
}

pub fn pool_state_pda(token_0: &Pubkey, token_1: &Pubkey) -> Result<Pubkey, UtilsError> {
    let (pool_state_account, _) = Pubkey::try_find_program_address(
        &[
            &crate::raydium::POOL_SEED.as_bytes(),
            &amm_config_pda()?.to_bytes(),
            &token_0.to_bytes(),
            &token_1.to_bytes(),
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?;
    Ok(pool_state_account)
}

pub fn lp_mint_pda(token_0: &Pubkey, token_1: &Pubkey) -> Result<Pubkey, UtilsError> {
    let (lp_mint, _) = Pubkey::try_find_program_address(
        &[
            &crate::raydium::POOL_LP_MINT_SEED.as_bytes(),
            &pool_state_pda(token_0, token_1)?.to_bytes(),
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?;
    Ok(lp_mint)
}

pub fn token_vault_pdas(token_0: &Pubkey, token_1: &Pubkey) -> Result<(Pubkey, Pubkey), UtilsError> {
    let (token_0_vault_account, _) = Pubkey::try_find_program_address(
        &[
            &crate::raydium::POOL_VAULT_SEED.as_bytes(),
            &pool_state_pda(token_0, token_1)?.to_bytes(),
            &token_0.to_bytes(),
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?;
    let (token_1_vault_account, _) = Pubkey::try_find_program_address(
        &[
            &crate::raydium::POOL_VAULT_SEED.as_bytes(),
            &pool_state_pda(token_0, token_1)?.to_bytes(),
            &token_1.to_bytes(),
        ],
        &crate::raydium::ID,
    ).ok_or_else(|| UtilsError::InvalidRaydiumProgramAccount)?;
    Ok((token_0_vault_account, token_1_vault_account))
}

#[cfg(test)]
mod test {
    use std::str::FromStr;
    use solana_program::pubkey::Pubkey;

    #[test]
    fn test_derive_raydium_amm_config_pda() {
        let pda = super::amm_config_pda().unwrap();
        assert_eq!(pda, Pubkey::from_str("DzyM4m7aC3Bg7AsuRs8kWDbuVADb9rbg1M68VEcFS86f").unwrap());
    }

    #[test]
    fn test_derive_raydium_observation_pda() {
        let token_0_mint = Pubkey::from_str("DVCdrDjyzda6Upb5W9Ayrs78Py5r6Xg8PuvXDtjngYLk").unwrap();
        let token_1_mint = Pubkey::from_str("EAkNjcMoZiz6K3qF1zDJjdqP5Y16gdFapmRJxgrjMKVp").unwrap();
        let pda = super::observation_pda(&token_0_mint, &token_1_mint).unwrap();
        assert_eq!(pda, Pubkey::from_str("6jzy18Td8QLKEmqy3LUJGEK6WDVRUZYLnn5uiAiXLoLe").unwrap());
    }

    #[test]
    fn test_derive_raydium_pool_state_pda() {
        let token_0_mint = Pubkey::from_str("DVCdrDjyzda6Upb5W9Ayrs78Py5r6Xg8PuvXDtjngYLk").unwrap();
        let token_1_mint = Pubkey::from_str("EAkNjcMoZiz6K3qF1zDJjdqP5Y16gdFapmRJxgrjMKVp").unwrap();
        let pda = super::pool_state_pda(&token_0_mint, &token_1_mint).unwrap();
        assert_eq!(pda, Pubkey::from_str("JDAQSvJVHr91h28RqNMkZrw7dUmVso1zfuuJVncNquj8").unwrap());
    }

    #[test]
    fn test_derive_raydium_token_vault_pda() {
        let token_0_mint = Pubkey::from_str("DVCdrDjyzda6Upb5W9Ayrs78Py5r6Xg8PuvXDtjngYLk").unwrap();
        let token_1_mint = Pubkey::from_str("EAkNjcMoZiz6K3qF1zDJjdqP5Y16gdFapmRJxgrjMKVp").unwrap();
        let pda = super::token_vault_pda(&token_0_mint, &token_0_mint, &token_1_mint).unwrap();
        assert_eq!(pda, Pubkey::from_str("8AvZdPUAEvXeTTGyUpBgfaGnAQsr5bapw8UU2LWnKbcz").unwrap());
    }

    #[test]
    fn test_derive_raydium_lp_mint_pda() {
        let token_0_mint = Pubkey::from_str("DVCdrDjyzda6Upb5W9Ayrs78Py5r6Xg8PuvXDtjngYLk").unwrap();
        let token_1_mint = Pubkey::from_str("EAkNjcMoZiz6K3qF1zDJjdqP5Y16gdFapmRJxgrjMKVp").unwrap();
        let pda = super::lp_mint_pda(&token_0_mint, &token_1_mint).unwrap();
        assert_eq!(pda, Pubkey::from_str("7Mi1NMPU31JPdyCpnuz7YeitUJwzrKN6SDZosvjUewHk").unwrap());
    }

    #[test]
    fn test_derive_raydium_authority_pda() {
        let pda = super::authority_pda().unwrap();
        assert_eq!(pda, Pubkey::from_str("3oTCJbHVWvPcuKkS9mAeqLEEy9BJ4Q6nNTWzPCm2LXy9").unwrap());
    }
}
