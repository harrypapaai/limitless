#![cfg(feature = "localnet")]

use solana_program::account_info::AccountInfo;
use blackwing_proc_macros::{ToAccountMetaList, account_infos_struct};
use utils::instructions::{ToAccountMetaList, AccountKey, WriteableAccountKey};
use crate::errors::LimitlessError;
use crate::instructions;
use crate::state::market::{MarketAccount, MarketAccountSignerSeeds};

#[account_infos_struct(MarketToDestTransferForTestAccountInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct MarketToDestTransferForTestAccounts {
    pub market_account: AccountKey,
    pub token_0_mint: AccountKey,
    pub token_1_mint: AccountKey,
    pub dest_transfer_token_ata: WriteableAccountKey,
    pub market_transfer_token_ata: WriteableAccountKey,
    pub transfer_token_mint: AccountKey,
    pub token_program: AccountKey,
    pub system_program: AccountKey,
}

struct SignerSeeds {
    market: MarketAccountSignerSeeds,
}

pub fn market_to_dest_transfer_for_test(
    accounts: &[AccountInfo],
    amt: u64,
) -> Result<(), LimitlessError> {
    let (account_infos, seeds) = parse_accounts(accounts)?;

    instructions::utils::transfer_spl_from_market_to_user(
        &seeds.market,
        account_infos.market_account,
        account_infos.market_transfer_token_ata,
        account_infos.dest_transfer_token_ata,
        account_infos.token_program,
        amt,
    )?;

    Ok(())
}

fn parse_accounts<'slice, 'info: 'slice>(
    accounts: &'slice [AccountInfo<'info>],
) -> Result<
    (MarketToDestTransferForTestAccountInfos<'slice, 'info>, SignerSeeds),
    LimitlessError
> {
    let account_info_iter = &mut accounts.iter();

    let market_account_info = utils::next_account_info(account_info_iter)?;
    let token_0_mint_account_info = utils::next_account_info(account_info_iter)?;
    let token_1_mint_account_info = utils::next_account_info(account_info_iter)?;
    let dest_transfer_token_ata_info = utils::next_account_info(account_info_iter)?;
    let market_transfer_token_ata_info = utils::next_account_info(account_info_iter)?;
    let transfer_token_mint_info = utils::next_account_info(account_info_iter)?;
    let token_program_info = utils::next_account_info(account_info_iter)?;
    let system_program_info = utils::next_account_info(account_info_iter)?;

    //
    // Validations.
    //

    utils::validate::validate_account(token_program_info, &spl_token::ID, LimitlessError::InvalidTokenProgramAccount)?;
    utils::validate::validate_account(system_program_info, &solana_program::system_program::ID, LimitlessError::InvalidSystemProgramAccount)?;

    utils::validate::validate_token_mint(
        token_0_mint_account_info,
        token_program_info,
        LimitlessError::InvalidToken0MintAccount,
    )?;
    utils::validate::validate_token_mint(
        token_1_mint_account_info,
        token_program_info,
        LimitlessError::InvalidToken1MintAccount,
    )?;
    utils::validate::validate_token_mint(
        transfer_token_mint_info,
        token_program_info,
        LimitlessError::InvalidTokenMintAccount,
    )?;

    // TODO: validate that the destination account is a valid token account.
    let market_transfer_token_account_signer_seeds = MarketAccount::token_account_signer_seeds(
        market_account_info.key,
        token_program_info.key,
        transfer_token_mint_info.key,
    );
    utils::validate::validate_pda(
        market_transfer_token_ata_info,
        token_program_info.key,
        &crate::ID,
        &market_transfer_token_account_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketRaydiumLpTokenAta,
    )?;

    let market_signer_seeds = MarketAccount::signer_seeds(
        token_0_mint_account_info.key,
        token_1_mint_account_info.key,
    )?;
    utils::validate::validate_pda_address(
        market_account_info,
        &crate::ID,
        &market_signer_seeds.as_refs(),
        LimitlessError::InvalidMarketAccountPda,
    )?;

    let accounts = MarketToDestTransferForTestAccountInfos::<'slice, 'info> {
        market_account: market_account_info,
        token_0_mint: token_0_mint_account_info,
        token_1_mint: token_1_mint_account_info,
        dest_transfer_token_ata: dest_transfer_token_ata_info,
        market_transfer_token_ata: market_transfer_token_ata_info,
        token_program: token_program_info,
        system_program: system_program_info,
        transfer_token_mint: transfer_token_mint_info,
    };

    Ok((accounts, SignerSeeds{
        market: market_signer_seeds,
    }))
}
