use anchor_lang::prelude::{AccountInfo, Pubkey};
use spl_associated_token_account::get_associated_token_address_with_program_id;
use crate::log;

pub fn validate_owner<T: std::error::Error + Copy>(
    account: &AccountInfo,
    expected_owner: &Pubkey,
    err: T,
) -> Result<(), T> {
    if !account.owner.eq(expected_owner) {
        return Err(err)
    };
    Ok(())
}

pub fn validate_signer<T: std::error::Error + Copy>(
    account: &AccountInfo,
    err: T,
) -> Result<(), T> {
    if !account.is_signer {
        return Err(err)
    };
    Ok(())
}

pub fn validate_pda<T: std::error::Error + Copy>(
    pda: &AccountInfo,
    owner: &Pubkey,
    program_account: &Pubkey,
    seeds: &[&[u8]],
    err: T
) -> Result<(), T> {
    if !pda.owner.eq(owner) {
        log!("Invalid PDA Owner for {}: actual: {} expected: {}", pda.key, pda.owner, program_account);
        return Err(err)
    };
    validate_pda_address(pda, program_account, seeds, err)
}

pub fn validate_pda_address<T: std::error::Error + Copy>(
    pda: &AccountInfo,
    program_account: &Pubkey,
    seeds: &[&[u8]],
    err: T
) -> Result<(), T> {
    let derived_pda = Pubkey::create_program_address(seeds, program_account)
        .map_err(|_| err)?;
    if pda.key != &derived_pda {
        log!("Invalid PDA Account: actual: {} expected: {}", pda.key, derived_pda);
        Err(err)
    } else {
        Ok(())
    }
}

pub fn validate_canonical_pda<T: std::error::Error + Copy>(
    pda: &AccountInfo,
    owner: &Pubkey,
    program_account: &Pubkey,
    seeds: &[&[u8]],
    err: T
) -> Result<(), T> {
    if !pda.owner.eq(owner) {
        log!("Invalid Canonical PDA Owner: actual: {} expected: {}", pda.owner, program_account);
        return Err(err)
    };
    validate_canonical_pda_address(pda, program_account, seeds, err)
}

pub fn validate_canonical_pda_address<T: std::error::Error + Copy>(
    pda: &AccountInfo,
    program_account: &Pubkey,
    seeds: &[&[u8]],
    err: T
) -> Result<(), T> {
    let (derived_pda, _) = Pubkey::find_program_address(seeds, program_account);
    if !pda.key.eq(&derived_pda) {
        log!("Invalid Canonical PDA address {:?} {:?}", pda.key, derived_pda);
        Err(err)
    } else {
        Ok(())
    }
}

pub fn validate_token_mint<T: std::error::Error>(
    mint: &AccountInfo,
    token_program: &AccountInfo,
    err: T
) -> Result<(), T> {
    if !mint.owner.eq(&token_program.key) {
        log!("Invalid Token Mint Owner: actual: {} expected: {}", mint.owner, token_program.key);
        return Err(err)
    };
    Ok(())
}

pub fn validate_ata<T: std::error::Error>(
    ata: &AccountInfo,
    token_mint: &AccountInfo,
    token_program: &AccountInfo,
    address: &Pubkey,
    err: T
) -> Result<(), T> {
    if !ata.owner.eq(token_program.key) {
        log!("Invalid ATA Owner for {}: actual: {} ata_program_id: {} spl_token_program_id: {}", ata.key, ata.owner, spl_associated_token_account::ID, spl_token::ID);
        return Err(err)
    };
    validate_ata_address(ata, token_mint, token_program, address, err)
}

pub fn validate_ata_address<T: std::error::Error>(
    ata: &AccountInfo,
    token_mint: &AccountInfo,
    token_program: &AccountInfo,
    address: &Pubkey,
    err: T
) -> Result<(), T> {
    let expected_ata = get_associated_token_address_with_program_id(
        address,
        token_mint.key,
        token_program.key,
    );
    if !ata.key.eq(&expected_ata) {
        log!("Invalid ATA Address: actual: {} expected: {}", ata.key, expected_ata);
        Err(err)
    } else {
        Ok(())
    }
}

pub fn validate_account<T: std::error::Error>(
    info: &AccountInfo,
    expected_account: &Pubkey,
    err: T
) -> Result<(), T> {
    if !info.key.eq(expected_account) {
        log!("Invalid Account Address: actual: {} expected: {}", info.key, expected_account);
        Err(err)
    } else {
        Ok(())
    }
}
