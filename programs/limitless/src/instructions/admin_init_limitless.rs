use anchor_lang::Key;
use solana_program::account_info::AccountInfo;
use blackwing_proc_macros::{account_infos_struct, ToAccountMetaList};
use utils::instructions::{ToAccountMetaList, AccountKey, SignerAccountKey, WriteableAccountKey};
use crate::errors::LimitlessError;
use crate::state::config::{ConfigAccount, ConfigAccountSignerSeeds, TradingMode};

#[account_infos_struct(AdminInitAccountInfos)]
#[derive(ToAccountMetaList, Debug)]
pub struct AdminInitAccounts {
    pub admin_account: SignerAccountKey,
    pub fee_collector_account: AccountKey,
    pub config_account: WriteableAccountKey,
    pub system_program: AccountKey,
    pub rent: AccountKey,
}

struct SignerSeeds {
    config_account: ConfigAccountSignerSeeds,
}

pub fn process_init_admin(accounts: &[AccountInfo]) -> Result<(), LimitlessError> {
    let (account_infos, seeds) = parse_accounts(accounts)?;

    let config = ConfigAccount {
        trading_mode: TradingMode::Enabled,
        fee_collector: account_infos.fee_collector_account.key(),
        space: [0; 128],
    };
    utils::state::create_account_with_signers(
        &config,
        account_infos.config_account,
        account_infos.admin_account,
        account_infos.rent,
        account_infos.system_program,
        &crate::ID,
        &[&seeds.config_account.as_refs()],
    ).map_err(|_| LimitlessError::CreateConfigAccountInvokeFailed)?;

    Ok(())
}

fn parse_accounts<'slice, 'info: 'slice>(accounts: &'slice [AccountInfo<'info>]) -> Result<
    (AdminInitAccountInfos<'slice, 'info>, SignerSeeds),
    LimitlessError
> {
    let account_info_iter = &mut accounts.iter();

    let admin_account_info = utils::next_account_info(account_info_iter)?;
    let fee_collector_info = utils::next_account_info(account_info_iter)?;
    let config_account_info = utils::next_account_info(account_info_iter)?;
    let system_program_info = utils::next_account_info(account_info_iter)?;
    let rent_info = utils::next_account_info(account_info_iter)?;

    //
    // Validations.
    //

    utils::validate::validate_signer(admin_account_info, LimitlessError::InvalidSigner)?;
    utils::validate::validate_account(admin_account_info, &crate::admin::ID, LimitlessError::InvalidAdmin)?;
    utils::validate::validate_account(system_program_info, &solana_program::system_program::ID, LimitlessError::InvalidSystemProgramAccount)?;
    utils::validate::validate_account(rent_info, &solana_program::sysvar::rent::ID, LimitlessError::InvalidRentAccount)?;

    let config_signer_seeds = ConfigAccount::signer_seeds()?;
    utils::validate::validate_pda_address(
        config_account_info,
        &crate::ID,
        &config_signer_seeds.as_refs(),
        LimitlessError::InvalidConfigAccountPda,
    )?;

    let accounts = AdminInitAccountInfos::<'slice, 'info> {
        admin_account: admin_account_info,
        fee_collector_account: fee_collector_info,
        config_account: config_account_info,
        system_program: system_program_info,
        rent: rent_info,
    };

    Ok((accounts, SignerSeeds{
        config_account: config_signer_seeds,
    }))
}
