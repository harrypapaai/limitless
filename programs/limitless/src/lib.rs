extern crate derive_more;

use anchor_lang::declare_id;

pub mod entrypoint;
pub mod processor;
pub mod calculator;
pub mod errors;
pub mod state;
pub mod instructions;
pub mod raydium;
pub mod pool;
pub mod client;
pub mod events;

#[cfg(feature = "localnet")]
declare_id!("6TvznH3B2e3p2mbhufNBpgSrLx6UkgvxtVQvopEZ2kuH");
#[cfg(feature = "devnet")]
declare_id!("DvTitXuLRxyKx48n8pWA8Fg19oJ2VPUxXHPNmuYiKdiJ");
#[cfg(not(any(feature = "localnet", feature = "devnet")))]
declare_id!("2Uz54UqcVgWHuLR3hd2WA4rkiGRbQd17m3VpNuDAdf8n");

pub mod admin {
    #[cfg(feature = "localnet")]
    solana_program::declare_id!("DQCb9TYafRVjJQ7aoSQtPZKzkJG6QeQhUswttURQhqws");

    #[cfg(feature = "devnet")]
    solana_program::declare_id!("HM5JQHxYhg7VUnXfPQGgsN4mKeVeJM8T2HVPpMrKEeQq");

    #[cfg(not(any(feature="devnet", feature="localnet")))]
    solana_program::declare_id!("DnwKiJuzG4jdG9bRt6qivnU95sEDevFKEA5PJnNMAHsy");
}
pub mod closer {
    #[cfg(feature = "localnet")]
    solana_program::declare_id!("E8avWXwHhButFq67iThJfJXt3e3GusUxWAj6J1MzutVu");

    #[cfg(feature = "devnet")]
    solana_program::declare_id!("GU8BMSapWCVA3uPLdVLCs3XZMUQcqy1tZWgqqte8k6u1");

    #[cfg(not(any(feature="devnet", feature="localnet")))]
    solana_program::declare_id!("7SUtHmoULYJBsYPfp2LqqYbJH9k2hWJMBN23NdYXDHNQ");
}
