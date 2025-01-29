pub mod raydium;
pub mod token;
pub mod anchor;

use std::fmt::Debug;
use std::io::Cursor;
use std::ops::Add;
use std::path::PathBuf;
use solana_program::instruction::{Instruction};
use solana_program::pubkey::Pubkey;
use solana_program_runtime::invoke_context::{BuiltinFunctionWithContext};
use solana_program_test::{BanksClient, BanksClientError, processor, ProgramTest, ProgramTestContext};
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use borsh::{BorshSerialize, BorshDeserialize};
use mpl_token_metadata::{
    ID as token_metadata_id,
};
use num_traits::ToPrimitive;
use solana_program::clock::Clock;
use solana_program::rent::Rent;
use solana_program_test::BanksClientError::TransactionError;
use solana_sdk::account::{Account, AccountSharedData};
use crate::raydium::instructions::RaydiumInitOpts;

pub const PROGRAM_FILE_DIRS: &'static [&str] = &[
    "fixtures",
    "test/fixtures",
    "src/fixtures",
    "../../tests/tutils/fixtures",
];

pub const USER_ACCOUNT_RENT_EXEMPTION: u64 = 1_000_000_000;

#[derive(BorshSerialize, BorshDeserialize)]
struct CreateAmmConfig{
    index: u16,
    trade_fee_rate: u64,
    protocol_fee_rate: u64,
    fund_fee_rate: u64,
    create_pool_fee: u64,
}

pub struct TBuilder {
    pt: ProgramTest,
    configure_raydium: bool,
    raydium_opts: Option<RaydiumInitOpts>,
}

pub fn new_t() -> TBuilder {
    let tb = TBuilder {
        pt: ProgramTest::default(),
        configure_raydium: false,
        raydium_opts: None,
    };
    tb.with_program(
        "spl_associated_token_account",
        spl_associated_token_account::ID,
        processor!(spl_associated_token_account::processor::process_instruction),
    )
}

pub fn load_limitless_admin_keypair() -> Keypair {
    let keypair_str = include_str!("../../tutils/fixtures/limitless_admin_keypair.json");
    let mut reader = Cursor::new(keypair_str);
    solana_sdk::signature::read_keypair(
        &mut reader
    ).unwrap()
}

pub fn load_limitless_closer_keypair() -> Keypair {
    let keypair_str = include_str!("../../tutils/fixtures/limitless_closer_keypair.json");
    let mut reader = Cursor::new(keypair_str);
    solana_sdk::signature::read_keypair(
        &mut reader
    ).unwrap()
}

pub fn load_creator_admin_keypair() -> Keypair {
    let keypair_str = include_str!("../../tutils/fixtures/creator_v2_admin_keypair.json");
    let mut reader = Cursor::new(keypair_str);
    solana_sdk::signature::read_keypair(
        &mut reader
    ).unwrap()
}


pub struct T {
    ctx: ProgramTestContext,
}

impl TBuilder {
    pub async fn build(self) -> T {
        let ctx = self.pt.start_with_context().await;
        let configure_raydium = self.configure_raydium;
        let mut t = T {
            ctx,
        };

        if configure_raydium {
            let rent = t.banks_client().get_rent().await.unwrap();
            let payer_pubkey = t.payer().pubkey();
            let (instructions, signers) = raydium::instructions::init(&payer_pubkey, rent, self.raydium_opts);
            let signer_refs = signers.iter().collect();
            t.tx(instructions, signer_refs).await;
        };

        t
    }

    pub fn with_program(
        mut self,
        name: &str,
        address: Pubkey,
        processor: Option<BuiltinFunctionWithContext>,
    ) -> TBuilder {
        if let Some(p) = processor {
            self.pt.add_builtin_program(
                name,
                address,
                p,
            );
        } else {
            let mut dirs: Vec<PathBuf> = Vec::from(PROGRAM_FILE_DIRS)
                .iter()
                .map(|e| PathBuf::from(e))
                .collect();
            if let Ok(curr_dir) = std::env::current_dir() {
                dirs.push(PathBuf::from(curr_dir))
            }
            let file = find_file(dirs, &format!("{}.so", name));
            if file.is_none() {
                panic!("Unable to find program file: {}", name);
            }
            let data = solana_program_test::read_file(file.unwrap());
            self.pt.add_account(
                address,
                Account {
                    lamports: Rent::default().minimum_balance(data.len()).max(1),
                    data,
                    owner: solana_sdk::bpf_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            );
        };
        self
    }

    pub fn with_raydium(mut self, opts: Option<RaydiumInitOpts>) -> TBuilder {
        self.configure_raydium = true;
        self.raydium_opts = opts;

        self.pt.prefer_bpf(true);

        self.with_program(
            "raydium_cp_swap",
            utils::raydium::ID,
            None,
        )
    }

    pub fn with_mpl_metadata(mut self) -> TBuilder {
        self.pt.prefer_bpf(true);

        self.with_program(
            "metaplex_token_metadata_program",
            token_metadata_id,
            None,
        )
    }
}

impl T {
    pub fn ctx(&mut self) -> &mut ProgramTestContext {
        &mut self.ctx
    }

    pub fn banks_client(&mut self) -> &mut BanksClient {
        &mut self.ctx.banks_client
    }

    pub fn payer(&self) -> &Keypair {
        &self.ctx.payer
    }

    pub async fn transfer_sol(self: &mut Self, to: &Pubkey, lamports_amt: u64) {
        let payer = self.payer().insecure_clone();
        let transfer_instruction = solana_program::system_instruction::transfer(
            &payer.pubkey(),
            to,
            lamports_amt,
        );
        let mut transaction = Transaction::new_with_payer(
            &[transfer_instruction],
            Some(&payer.pubkey()),
        );
        transaction.sign(
            &[&payer],
            self.banks_client().get_latest_blockhash().await.unwrap(),
        );
        self.banks_client().process_transaction(transaction).await.unwrap();
    }

    pub async fn get_account<T: BorshDeserialize>(self: &mut Self, key: &Pubkey) -> T {
        let acc = self.banks_client().get_account(key.clone()).await.unwrap().unwrap();
        T::try_from_slice(&acc.data).unwrap()
    }

    pub async fn write_account<T: BorshDeserialize + BorshSerialize>(self: &mut Self, key: &Pubkey, data: &T) {
        let mut acc = self.banks_client().get_account(key.clone()).await.unwrap().unwrap();
        acc.data.clear();
        data.serialize(&mut acc.data).unwrap();
        self.ctx.set_account(key, &AccountSharedData::from(acc));
    }

    pub async fn tx(self: &mut Self, inst: Vec<Instruction>, signers: Vec<&Keypair>) {
        self.tx_res(inst, signers).await.unwrap();
    }

    pub async fn tx_res(self: &mut Self, inst: Vec<Instruction>, signers: Vec<&Keypair>) -> Result<(), BanksClientError> {
        let tx = self.build_tx(inst, signers).await;
        self.banks_client().process_transaction(tx).await
    }

    async fn build_tx(self: &mut Self, inst: Vec<Instruction>, signers: Vec<&Keypair>) -> Transaction {
        let payer = self.payer().insecure_clone();
        let mut transaction = Transaction::new_with_payer(
            inst.as_slice(),
            Some(&payer.pubkey()),
        );
        transaction.sign(
            [&[&payer], signers.as_slice()].concat().iter().as_slice(),
            self.banks_client().get_latest_blockhash().await.unwrap(),
        );
        transaction
    }

    pub async fn move_time_fwd(&mut self, time_seconds: u64) {
        let clock_sysvar: Clock = self.banks_client().get_sysvar().await.unwrap();
        println!(
            "Original Time: epoch = {}, timestamp = {}",
            clock_sysvar.epoch, clock_sysvar.unix_timestamp
        );
        let mut new_clock = clock_sysvar.clone();
        new_clock.epoch = new_clock.epoch + 30;
        new_clock.unix_timestamp = new_clock.unix_timestamp.add(time_seconds as i64);

        self.ctx.set_sysvar(&new_clock);
        let clock_sysvar: Clock = self.banks_client().get_sysvar().await.unwrap();
        println!(
            "New Time: epoch = {}, timestamp = {}",
            clock_sysvar.epoch, clock_sysvar.unix_timestamp
        );
    }

    pub async fn set_slot(&mut self, new_slot: u64) {
        let clock_sysvar: Clock = self.banks_client().get_sysvar().await.unwrap();
        assert!(new_slot > clock_sysvar.slot);
        println!("Original Slot: {}", clock_sysvar.slot);
        let mut new_clock = clock_sysvar.clone();
        new_clock.slot = new_slot;
        self.ctx.set_sysvar(&new_clock);
        let clock_sysvar: Clock = self.banks_client().get_sysvar().await.unwrap();
        println!("New Slot: {}", clock_sysvar.slot);
    }
}

pub fn assert_err<T, R>(res: Result<R, BanksClientError>, program_err: T) where T: ToPrimitive + Debug {
    if let Err(TransactionError(e)) = res {
        if let solana_sdk::transaction::TransactionError::InstructionError(_, ie) = e {
            assert_eq!(ie, solana_program::instruction::InstructionError::Custom(program_err.to_u32().unwrap()));
        }
    } else {
        panic!("Expected error: {:?}", program_err);
    }
}

fn find_file(dirs: Vec<PathBuf>, filename: &str) -> Option<PathBuf> {
    for dir in dirs {
        let candidate = dir.join(filename);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}
