use anchor_lang::AccountDeserialize;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use spl_token::instruction::{mint_to, sync_native};
use spl_token::state::{Account as TokenAccount, Mint};
use anchor_spl::token::TokenAccount as AnchorTokenAccount;
use crate::*;

pub async fn create_mock_token_pair(t: &mut T) -> (Keypair, Keypair) {
    let mut token_0 = create_mock_token(t, 9).await;
    let mut token_1 = create_mock_token(t, 9).await;

    if token_1.pubkey() <= token_0.pubkey()  {
        let temp = token_0.insecure_clone();
        token_0 = token_1;
        token_1 = temp;
    }

    (token_0, token_1)
}

pub async fn create_mock_token(t: &mut T, decimals: u8) -> Keypair {
    let keypair = Keypair::new();

    let rent = t.banks_client().get_rent().await.unwrap();

    let init_account_ix = solana_program::system_instruction::create_account(
        &t.payer().pubkey(),
        &keypair.pubkey(),
        rent.minimum_balance(Mint::LEN),
        Mint::LEN as u64,
        &spl_token::ID,
    );
    let init_mint_ix = spl_token::instruction::initialize_mint(
        &spl_token::ID,
        &keypair.pubkey(),
        &keypair.pubkey(),
        Some(&keypair.pubkey()),
        decimals,
    ).unwrap();
    t.tx(
        vec![init_account_ix, init_mint_ix],
        vec![&keypair],
    ).await;

    keypair
}

pub async fn create_token_account(t: &mut T, owner: &Pubkey, mint: &Pubkey) {
    let create_account_tx = create_associated_token_account_idempotent(
        &t.payer().pubkey(),
        owner,
        mint,
        &spl_token::ID,
    );
    t.tx(
        vec![create_account_tx],
        vec![],
    ).await;
}

pub async fn mint_mock_token(
    t: &mut T,
    mock_token: &Keypair,
    dest: &Pubkey,
    amount: u64,
) {
    let ata = get_associated_token_address(dest, &mock_token.pubkey());

    let create_ata_ix = create_associated_token_account_idempotent(
        &t.payer().pubkey(),
        dest,
        &mock_token.pubkey(),
        &spl_token::ID,
    );

    let mint_to_ix = mint_to(
        &spl_token::ID,
        &mock_token.pubkey(),
        &ata,
        &mock_token.pubkey(),
        &[&t.payer().pubkey(), &mock_token.pubkey()],
        amount,
    ).unwrap();
    t.tx(
        vec![create_ata_ix, mint_to_ix],
        vec![&mock_token],
    ).await;
}

pub async fn mint_mock_token_to_account(
    t: &mut T,
    mock_token: &Keypair,
    dest: &Pubkey,
    amount: u64,
) {
    let mint_to_ix = mint_to(
        &spl_token::ID,
        &mock_token.pubkey(),
        &dest,
        &mock_token.pubkey(),
        &[&t.payer().pubkey(), &mock_token.pubkey()],
        amount,
    ).unwrap();
    t.tx(
        vec![mint_to_ix],
        vec![&mock_token],
    ).await;
}

pub async fn get_lamports(
    t: &mut T,
    dest: &Pubkey,
    amount: u64,
) {
    let transfer_ix = solana_program::system_instruction::transfer(
        &t.payer().pubkey(),
        dest,
        amount
    );
    t.tx(
        vec![transfer_ix],
        vec![],
    ).await;
}

pub async fn get_wsol(
    t: &mut T,
    dest: &Pubkey,
    amount: u64,
) {
    create_token_account(t, dest, &spl_token::native_mint::ID).await;

    let wsol_ata = get_associated_token_address(dest, &spl_token::native_mint::ID);
    get_lamports(t, &wsol_ata, amount).await;

    let sync_ix = sync_native(&spl_token::ID, &wsol_ata).unwrap();
    t.tx(
        vec![sync_ix],
        vec![],
    ).await;
}

pub async fn get_wsol_to_account(
    t: &mut T,
    dest: &Pubkey,
    amount: u64,
) {
    get_lamports(t, &dest, amount).await;

    let sync_ix = sync_native(&spl_token::ID, &dest).unwrap();
    t.tx(
        vec![sync_ix],
        vec![],
    ).await;
}

pub async fn get_token_balance(
    t: &mut T,
    token_mint_account: &Pubkey,
    holder: &Pubkey,
) -> u64 {
    get_token_account(t, token_mint_account, holder).await.amount
}

pub async fn get_token_account(
    t: &mut T,
    token_mint_account: &Pubkey,
    holder: &Pubkey,
) -> TokenAccount {
    let ata_token_account_address = get_associated_token_address(holder, token_mint_account);
    let account = t.banks_client().get_account(ata_token_account_address).await.unwrap().unwrap();
    TokenAccount::unpack(&account.data).unwrap()
}

pub async fn get_token_account_lamports(
    t: &mut T,
    token_mint_account: &Pubkey,
    holder: &Pubkey,
) -> u64 {
    let ata_token_account_address = get_associated_token_address(holder, token_mint_account);
    t.banks_client().get_balance(ata_token_account_address).await.unwrap()
}

pub async fn get_token_balance_from_token_account(t: &mut T, ata: &Pubkey) -> u64 {
    let account = t.banks_client().get_account(ata.clone()).await.unwrap().unwrap();
    TokenAccount::unpack(&account.data).unwrap().amount
}

pub async fn get_token_balance_from_anchor_anchor_token_account(t: &mut T, ata: &Pubkey) -> u64 {
    let account = t.banks_client().get_account(ata.clone()).await.unwrap().unwrap();
    AnchorTokenAccount::try_deserialize(&mut account.data.as_slice()).unwrap().amount
}

pub async fn get_token_mint(t: &mut T, mint_account: &Pubkey) -> Mint {
    let account = t.banks_client().get_account(mint_account.clone()).await.unwrap().unwrap();
    Mint::unpack(&account.data).unwrap()
}

pub async fn transfer_tokens(
    t: &mut T,
    from_authority: &Keypair,
    from_token_account: &Pubkey,
    to_token_account: &Pubkey,
    amount: u64,
) {
    let transfer_ix = spl_token::instruction::transfer(
        &spl_token::ID,
        from_token_account,
        to_token_account,
        &from_authority.pubkey(),
        &[],
        amount,
    ).unwrap();
    t.tx(
        vec![transfer_ix],
        vec![from_authority],
    ).await;
}

pub struct TokenMinter {
    keypair: Keypair,
    is_native: bool
}

impl TokenMinter {
    pub fn new(keypair: Keypair) -> Self {
        TokenMinter {
            keypair,
            is_native: false,
        }
    }

    pub fn new_wsol() -> Self {
        let keypair = Keypair::new();
        TokenMinter {
            keypair,
            is_native: true,
        }
    }

    pub fn pubkey(&self) -> Pubkey {
        if self.is_native {
            return spl_token::native_mint::ID
        } else {
            return self.keypair.pubkey()
        }
    }

    pub async fn mint(&self, t: &mut T, dest: &Pubkey, amount: u64) {
        if self.is_native {
            get_wsol(t, dest, amount).await
        } else {
            mint_mock_token(t, &self.keypair, dest, amount).await
        }
    }

    pub async fn mint_to_account(&self, t: &mut T, dest: &Pubkey, amount: u64) {
        if self.is_native {
            get_wsol_to_account(t, dest, amount).await
        } else {
            mint_mock_token_to_account(t, &self.keypair, dest, amount).await
        }
    }
}

