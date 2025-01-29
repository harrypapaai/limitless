use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;

pub trait ToAccountMetaList {
    fn to_account_meta_list(&self) -> Vec<AccountMeta>;
}

#[derive(Debug, Clone, Copy)]
pub struct DynamicAccountKey {
    pub key: Pubkey,
    pub is_signer: bool,
    pub is_writeable: bool,
}

impl DynamicAccountKey {
    pub fn new(key: Pubkey, is_writeable: bool, is_signer: bool, ) -> Self {
        Self {
            key,
            is_signer,
            is_writeable,
        }
    }
}

impl Into<Pubkey> for DynamicAccountKey {
    fn into(self) -> Pubkey {
        self.key
    }
}

impl Into<AccountMeta> for DynamicAccountKey {
    fn into(self) -> AccountMeta {
        if self.is_writeable {
            AccountMeta::new(self.key, self.is_signer)
        } else {
            AccountMeta::new_readonly(self.key, self.is_signer)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WriteableSignerAccountKey {
    pub key: Pubkey,
}

impl From<Pubkey> for WriteableSignerAccountKey {
    fn from(value: Pubkey) -> Self {
        Self { key: value }
    }
}

impl From<&Pubkey> for WriteableSignerAccountKey {
    fn from(value: &Pubkey) -> Self {
        Self { key: value.clone() }
    }
}

impl Into<Pubkey> for WriteableSignerAccountKey {
    fn into(self) -> Pubkey {
        self.key
    }
}

impl Into<AccountMeta> for WriteableSignerAccountKey {
    fn into(self) -> AccountMeta {
        AccountMeta::new(self.key, true)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SignerAccountKey {
    pub key: Pubkey,
}

impl From<Pubkey> for SignerAccountKey {
    fn from(value: Pubkey) -> Self {
        Self { key: value }
    }
}

impl From<&Pubkey> for SignerAccountKey {
    fn from(value: &Pubkey) -> Self {
        Self { key: value.clone() }
    }
}

impl Into<Pubkey> for SignerAccountKey {
    fn into(self) -> Pubkey {
        self.key
    }
}

impl Into<AccountMeta> for SignerAccountKey {
    fn into(self) -> AccountMeta {
        AccountMeta::new(self.key, true)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WriteableAccountKey {
    pub key: Pubkey,
}

impl From<Pubkey> for WriteableAccountKey {
    fn from(value: Pubkey) -> Self {
        Self { key: value }
    }
}

impl From<&Pubkey> for WriteableAccountKey {
    fn from(value: &Pubkey) -> Self {
        Self { key: value.clone() }
    }
}

impl Into<Pubkey> for WriteableAccountKey {
    fn into(self) -> Pubkey {
        self.key
    }
}

impl Into<AccountMeta> for WriteableAccountKey {
    fn into(self) -> AccountMeta {
        AccountMeta::new(self.key, false)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AccountKey {
    pub key: Pubkey,
}

impl From<Pubkey> for AccountKey {
    fn from(value: Pubkey) -> Self {
        Self { key: value }
    }
}

impl From<&Pubkey> for AccountKey {
    fn from(value: &Pubkey) -> Self {
        Self { key: value.clone() }
    }
}

impl Into<Pubkey> for AccountKey {
    fn into(self) -> Pubkey {
        self.key
    }
}

impl Into<AccountMeta> for AccountKey {
    fn into(self) -> AccountMeta {
        AccountMeta::new_readonly(self.key, false)
    }
}
