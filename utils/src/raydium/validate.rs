use anchor_lang::prelude::AccountInfo;
use crate::errors::UtilsError;
use crate::log;
use crate::raydium::{self, raydium_cp_swap};
use crate::validate::*;
use crate::state::*;

pub fn validate_raydium_accounts_for_pool_state(
    token_0_mint_info: &AccountInfo,
    token_1_mint_info: &AccountInfo,
    raydium_program_info: &AccountInfo,
    raydium_config_info: &AccountInfo,
    pool_state_info: &AccountInfo,
    pool_authority_info: Option<&AccountInfo>,
    pool_observation_info: Option<&AccountInfo>,
) -> Result<raydium_cp_swap::accounts::PoolState, UtilsError> {
    if !raydium_program_info.key.eq(&raydium::ID) {
        log!("Invalid Raydium Program ID: actual: {:?} expected: {:?}", raydium_program_info.key, raydium::ID);
        return Err(UtilsError::InvalidRaydiumProgramAccount);
    };
    validate_canonical_pda(
        raydium_config_info,
        raydium_program_info.key,
        raydium_program_info.key,
        &[
            &raydium::AMM_CONFIG_SEED.as_bytes(),
            &raydium::INDEX.to_be_bytes(), // index.
        ],
        UtilsError::InvalidRaydiumConfigPda,
    )?;
    validate_canonical_pda(
        pool_state_info,
        raydium_program_info.key,
        raydium_program_info.key,
        &[
            &raydium::POOL_SEED.as_bytes(),
            &raydium_config_info.key.to_bytes(),
            &token_0_mint_info.key.to_bytes(),
            &token_1_mint_info.key.to_bytes(),
        ],
        UtilsError::InvalidRaydiumPoolStatePda,
    )?;
    if let Some(info) = pool_authority_info {
        validate_canonical_pda_address(
            info,
            raydium_program_info.key,
            &[
                &raydium::AUTH_SEED.as_bytes(),
            ],
            UtilsError::InvalidRaydiumPoolAuthorityPda,
        )?;
    }
    if let Some(info) = pool_observation_info {
        validate_canonical_pda(
            info,
            raydium_program_info.key,
            raydium_program_info.key,
            &[
                &raydium::OBSERVATION_SEED.as_bytes(),
                &pool_state_info.key.to_bytes(),
            ],
            UtilsError::InvalidRaydiumPoolObservationStatePda,
        )?;
    }

    let pool_state = anchor_unpack_info::<raydium_cp_swap::accounts::PoolState>(pool_state_info)
        .map_err(|_| UtilsError::RaydiumPoolStateSerializationFailed)?;

    Ok(pool_state)
}

pub fn validate_raydium_accounts_for_new_pool(
    token_0_mint_info: &AccountInfo,
    token_1_mint_info: &AccountInfo,
    token_0_vault_info: &AccountInfo,
    token_1_vault_info: &AccountInfo,
    lp_mint_info: &AccountInfo,
    raydium_program_info: &AccountInfo,
    raydium_config_info: &AccountInfo,
    pool_state_info: &AccountInfo,
    pool_authority_info: &AccountInfo,
    pool_observation_info: &AccountInfo,
) -> Result<raydium_cp_swap::accounts::AmmConfig, UtilsError> {
    if !raydium_program_info.key.eq(&raydium::ID) {
        log!("Invalid Raydium Program ID: actual: {:?} expected: {:?}", raydium_program_info.key, raydium::ID);
        return Err(UtilsError::InvalidRaydiumProgramAccount);
    };
    validate_canonical_pda_address(
        token_0_vault_info,
        raydium_program_info.key,
        &[
            &raydium::POOL_VAULT_SEED.as_bytes(),
            &pool_state_info.key.to_bytes(),
            &token_0_mint_info.key.to_bytes(),
        ],
        UtilsError::InvalidRaydiumToken0VaultAccount,
    )?;
    validate_canonical_pda_address(
        token_1_vault_info,
        raydium_program_info.key,
        &[
            &raydium::POOL_VAULT_SEED.as_bytes(),
            &pool_state_info.key.to_bytes(),
            &token_1_mint_info.key.to_bytes(),
        ],
        UtilsError::InvalidRaydiumToken1VaultAccount,
    )?;
    validate_canonical_pda_address(
        lp_mint_info,
        raydium_program_info.key,
        &[
            &raydium::POOL_LP_MINT_SEED.as_bytes(),
            &pool_state_info.key.to_bytes(),
        ],
        UtilsError::InvalidRaydiumLpMintAccount,
    )?;
    validate_canonical_pda(
        raydium_config_info,
        raydium_program_info.key,
        raydium_program_info.key,
        &[
            &raydium::AMM_CONFIG_SEED.as_bytes(),
            &raydium::INDEX.to_be_bytes(), // index.
        ],
        UtilsError::InvalidRaydiumConfigPda,
    )?;
    validate_canonical_pda_address(
        pool_state_info,
        raydium_program_info.key,
        &[
            &raydium::POOL_SEED.as_bytes(),
            &raydium_config_info.key.to_bytes(),
            &token_0_mint_info.key.to_bytes(),
            &token_1_mint_info.key.to_bytes(),
        ],
        UtilsError::InvalidRaydiumPoolStatePda,
    )?;
    validate_canonical_pda_address(
        pool_authority_info,
        raydium_program_info.key,
        &[
            &raydium::AUTH_SEED.as_bytes(),
        ],
        UtilsError::InvalidRaydiumPoolAuthorityPda,
    )?;
    validate_canonical_pda(
        pool_observation_info,
        raydium_program_info.key,
        raydium_program_info.key,
        &[
            &raydium::OBSERVATION_SEED.as_bytes(),
            &pool_state_info.key.to_bytes(),
        ],
        UtilsError::InvalidRaydiumPoolObservationStatePda,
    )?;

    let amm_config = anchor_unpack_info::<raydium_cp_swap::accounts::AmmConfig>(raydium_config_info)
        .map_err(|_| UtilsError::RaydiumAmmConfigSerializationFailed)?;

    Ok(amm_config)
}
