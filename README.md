# Limitless Pools

Liquidation-free leveraged trading on Solana, powered by concentrated liquidity market maker (CLMM) pools.

## What are Limitless Pools?

Limitless Pools are a novel DeFi primitive that enables leveraged trading without liquidations. Instead of traditional margin-based liquidation mechanics, positions are backed by LP tokens from an underlying CLMM pool (Raydium).

**How it works:**

- **Traders** open leveraged long or short positions by borrowing LP tokens from the pool. The LP tokens are decomposed to isolate directional exposure. Positions have a fixed duration (in slots) and pay a fee to LPs.
- **Liquidity Providers** deposit LP tokens (or mint-and-deposit in one step) to earn fees from traders. Fees accrue proportionally via a share-based pool system.
- **No liquidations** — positions expire at their `close_block` and can be closed by anyone after expiry. Traders can also close early. Rollover extends the position duration.
- **Domain premiums** — the borrowing fee is a function of the base fee APR and pool utilization, ensuring LPs are compensated for higher demand.

## Integration

The limitless program exposes instruction builders for CPI and client integration. All builders return `Result<Instruction, LimitlessError>`.

### Init Market

Creates a new trading market for a token pair.

```rust
use limitless::instructions::{init_market_ix, InitMarketArgs};
use limitless::state::market::{QuoteToken, TradingMode};

let ix = init_market_ix(
    &user,              // signer, market creator
    &token_0_mint,
    &token_1_mint,
    &fee_collector,     // receives protocol fees
    InitMarketArgs {
        trading_mode: TradingMode::Enabled,
        quote_token: QuoteToken::Token0,
        base_fee_apr: 100_000,        // base annual fee rate
        min_fee_quote_token: 1_000,   // minimum fee per position
        min_duration_slots: 10,
        max_duration_slots: 1_000_000,
    },
)?;
```

### Open Position

Opens a leveraged long or short position.

```rust
use limitless::instructions::{open_position_ix, OpenPositionArgs};

let ix = open_position_ix(
    &user,
    &token_0_mint,
    &token_1_mint,
    OpenPositionArgs {
        id: uuid::Uuid::new_v4(),
        collateral_quote_token_amt: 9_900_990_099,
        is_short: false,
        duration_blocks: 1_000_000,
        max_raydium_fee_quote_token_amt: 10_000_000_000,
        max_blackwing_fee_quote_token_amt: 0,
        rollover_reserve_quote_token_amt: 0,
        worst_price_num: 1050,
        worst_price_den: 1,
    },
)?;
```

### Close Position

Closes an existing position. Can be called by the owner at any time, or by anyone after expiry.

```rust
use limitless::instructions::{close_position_ix, ClosePositionArgs};
use limitless::state::market::QuoteToken;

let ix = close_position_ix(
    &user,              // position owner
    &payer,             // tx payer (can differ from owner)
    true,               // is_owner
    &token_0_mint,
    &token_1_mint,
    &fee_collector,
    QuoteToken::Token0,
    ClosePositionArgs {
        id: position_id,
        worst_price_num: 1050,
        worst_price_den: 1,
    },
)?;
```

### Deposit Liquidity

Deposits LP tokens into a market to earn trading fees. Supports both pre-minted LP tokens and mint-and-deposit in a single instruction.

```rust
use limitless::instructions::{deposit_liquidity_ix, DepositLpTokensArgs};

let ix = deposit_liquidity_ix(
    &user,
    &token_0_mint,
    &token_1_mint,
    &user_token_0_ata,
    &user_token_1_ata,
    DepositLpTokensArgs::MintLpTokensAndDeposit {
        lp_token_amt: 1_000_000,
        max_token_0_amt: 500_000,
        max_token_1_amt: 500_000,
    },
)?;
```

### Withdraw Liquidity

Withdraws LP tokens and accrued fees from a market.

```rust
use limitless::instructions::{withdraw_liquidity_ix, WithdrawLpTokensArgs};

let ix = withdraw_liquidity_ix(
    &user,
    &token_0_mint,
    &token_1_mint,
    &user_token_0_ata,
    &user_token_1_ata,
    WithdrawLpTokensArgs::WithdrawAllLpTokens {
        min_received_lp_tokens: 0,
        burn_max: true,
    },
)?;
```

### Rollover Position

Extends an active position's duration. Can optionally override the fee and duration.

```rust
use limitless::instructions::{rollover_position_ix, RolloverPositionArgs};
use limitless::state::market::QuoteToken;

let ix = rollover_position_ix(
    &user,
    &token_0_mint,
    &token_1_mint,
    &fee_collector,
    QuoteToken::Token0,
    RolloverPositionArgs {
        id: position_id,
        max_fee_override_quote_token_amt: None,
        rollover_duration_override: None,
    },
)?;
```

### Key Data Structures

```rust
// Market account — PDA seeds: ["market_account", token_0_mint, token_1_mint]
pub struct MarketAccount {
    pub trading_mode: TradingMode,       // Enabled, PositionCloseOnly, Disabled
    pub quote_token: QuoteToken,         // Token0 or Token1
    pub base_fee_apr: u64,
    pub min_fee_quote_token: u64,
    pub min_duration_slots: u64,
    pub max_duration_slots: u64,
    pub lp_tokens_supplied_pool: Pool,
    pub lp_tokens_removed_for_positions: u64,
    pub creator: Pubkey,
    // ... fee balance pools, extra space
}

// Position account — PDA seeds: ["position_account", market, user, position_id]
pub struct PositionAccount {
    pub id: Uuid,
    pub market_account: Pubkey,
    pub position_size: u64,
    pub is_short: bool,
    pub open_block: u64,
    pub close_block: u64,
    pub lp_tokens_removed: u64,
    pub base_token_balance: u64,
    pub quote_token_balance: u64,
    // ... collateral, fees, price snapshots, rollover settings
}

// Liquidity position — PDA seeds: ["liquidity_position_account", market, user]
pub struct LiquidityPositionAccount {
    pub lp_token_pool_position: PoolPosition,
    pub lp_token_fee_pool_position: BalancePoolPosition,
    pub token_0_fee_pool_position: BalancePoolPosition,
    pub token_1_fee_pool_position: BalancePoolPosition,
}
```

## Build & Test

Requires the [Solana tool suite](https://docs.solana.com/cli/install-solana-cli-tools) and Rust.

```shell
# Build the program (localnet)
make build

# Build for production
make build-prod

# Run tests
make tests
```
