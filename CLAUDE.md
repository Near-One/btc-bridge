# CLAUDE.md - Instructions for AI Assistant

This file contains important context and guidelines for working with the NEAR BTC Bridge codebase.

---

## Project Overview

**NEAR BTC Bridge** is a trustless bridge between Bitcoin and NEAR Protocol that allows users to:
- Deposit BTC and receive nBTC (fungible token on NEAR)
- Withdraw nBTC and receive BTC back
- Manage Bitcoin UTXOs efficiently

**Key Components:**
- `contracts/nbtc/` - NEP-141 fungible token contract (nBTC)
- `contracts/satoshi-bridge/` - Main bridge contract managing deposits/withdrawals
- `contracts/mock-*` - Testing utilities

**External Dependencies:**
- BTC Light Client contract (SPV verification)
- Chain Signatures contract (MPC for Bitcoin signing)

---

## Architecture & Key Concepts

### CRITICAL: Understanding the Flow

#### Deposit Flow (BTC → nBTC)
```
1. User sends BTC to deposit address (derived from DepositMsg hash)
2. Relayer calls: bridge.verify_deposit(tx_proof)
3. Bridge verifies transaction with BTC Light Client
4. Bridge calls: nbtc.mint(user, amount)
5. nBTC mints tokens to user
6. UTXO added to bridge's available set
```

#### Withdraw Flow (nBTC → BTC)
```
1. User calls: nbtc.ft_transfer(bridge, amount, WithdrawMsg)
   → Tokens TRANSFERRED to bridge (not burned yet!)
2. nBTC calls: bridge.ft_on_transfer(user, amount, msg)
   → Bridge returns 0 (keeps all tokens)
3. Bridge creates pending BTC transaction
4. Chain Signatures signs transaction
5. Transaction broadcast to Bitcoin network
6. Relayer calls: bridge.verify_withdraw(tx_proof)
7. Bridge verifies with Light Client
8. Bridge calls: nbtc.burn(user, amount, relayer, fee)
   → Burns from bridge balance (tokens already there!)
```

**IMPORTANT:** The `burn_account_id` parameter in `burn()` is for ACCOUNTING/EVENTS only. The actual burn happens from `bridge_id` balance where tokens already are after `ft_transfer`.

### Token Flow Understanding

```
┌─────────────────────────────────────────────────────┐
│              nBTC Token Contract                     │
│                                                      │
│  User Balance: 1000                                 │
│  Bridge Balance: 0                                  │
│                                                      │
│         ↓ ft_transfer(bridge, 100, msg)            │
│                                                      │
│  User Balance: 900                                  │
│  Bridge Balance: 100  ← TOKENS HERE NOW            │
│                                                      │
│         ↓ ft_on_transfer callback                   │
│         ↓ (bridge returns 0 = keep tokens)          │
│                                                      │
│  After BTC tx verified:                             │
│         ↓ burn(user, 100, ...)                      │
│                                                      │
│  Bridge Balance: 0    ← BURNED FROM HERE            │
│  Total Supply: 900                                  │
└─────────────────────────────────────────────────────┘
```

---

## Security Guidelines

### 🔒 NEVER DO

1. **DON'T suggest reading or modifying:**
   - `.env` files
   - `config.*` files
   - Any file starting with `.`
   - Private keys or secrets

2. **DON'T propose changes that:**
   - Remove `assert_one_yocto()` protections
   - Bypass `#[private]` callbacks
   - Disable access control checks
   - Skip `assert_bridge()` or `assert_controller()` checks

3. **DON'T create commits without explicit user request:**
   - User must specifically ask to commit
   - Always run tests before committing
   - Never use `--no-verify`

4. **DON'T push to remote unless explicitly requested:**
   - NEVER push to main/omni-main without confirmation
   - NEVER force push without explicit request

### ✅ ALWAYS DO

1. **Security patterns to preserve:**
   ```rust
   // Always use checked arithmetic for money operations
   amount.checked_mul(rate).unwrap_or_else(|| env::panic_str("overflow"))

   // Always validate before external calls
   require!(is_valid, "Invalid input");

   // Always use #[private] for callbacks
   #[private]
   pub fn callback(...) { }
   ```

2. **Access control patterns:**
   ```rust
   // Bridge can only call nBTC
   self.assert_bridge();

   // Controller can modify config
   self.assert_controller();

   // Use near-plugins decorators
   #[access_control_any(roles(Role::DAO))]
   ```

3. **Before suggesting changes:**
   - Understand the full flow (deposit or withdraw)
   - Check if tokens are already transferred
   - Verify callback execution order
   - Consider NEAR execution model (atomic callbacks)

---

## Code Conventions

### Rust Style

```rust
// Use descriptive names
pub fn internal_mint_promise(...) -> Promise { }  // Good
pub fn imp(...) -> Promise { }                     // Bad

// Explicit error messages
require!(condition, "Clear error message describing what went wrong");

// Use checked arithmetic for financial operations
let fee = amount.checked_mul(rate).and_then(|v| v.checked_div(MAX_RATIO))
    .unwrap_or_else(|| env::panic_str("Fee calculation overflow"));
```

### NEAR-Specific Patterns

```rust
// Callbacks must be #[private]
#[private]
pub fn callback_after_external_call(...) { }

// Use near-plugins for access control
#[access_control_any(roles(Role::DAO, Role::Operator))]
pub fn admin_function(...) { }

// Pausable functions
#[pause(except(roles(Role::DAO)))]
pub fn user_function(...) { }

// Assert one yocto for security
#[payable]
pub fn sensitive_operation(...) {
    assert_one_yocto();
    // ...
}
```

### Event Emission

```rust
// Emit events AFTER state changes
self.internal_set_utxo(&key, utxo);
Event::UtxoAdded { utxo_storage_keys: vec![key] }.emit();

// NOT before:
let event = Event::UtxoAdded { ... };  // ❌ Created too early
// ... state changes ...
event.emit();
```

---

## Testing Approach

### Running Tests

```bash
# Build all contracts
make build

# Run tests
cargo test

# Run specific test
cargo test test_name

# Check formatting
cargo fmt --check

# Clippy
cargo clippy -- -D warnings
```

### Test Structure

```rust
#[test]
fn test_deposit_flow() {
    // 1. Setup
    let context = get_context();
    testing_env!(context);
    let mut contract = Contract::new(...);

    // 2. Execute
    let result = contract.verify_deposit(...);

    // 3. Verify
    assert_eq!(result.is_success(), true);
    assert_eq!(contract.get_balance(), expected);
}
```

### Mock Contracts

Use mock contracts in `contracts/mock-*` for testing:
- `mock-btc-light-client` - Simulates BTC SPV verification
- `mock-chain-signatures` - Simulates MPC signing
- `mock-dapp` - Simulates external contract for post_actions

---

## Common Workflows

### Adding a New Configuration Parameter

1. Add field to `Config` struct:
   ```rust
   pub struct Config {
       // ... existing fields
       pub new_parameter: u64,
   }
   ```

2. Add validation in `assert_valid()`:
   ```rust
   impl Config {
       pub fn assert_valid(&self) {
           // ... existing validations
           require!(self.new_parameter > 0, "Invalid new_parameter");
       }
   }
   ```

3. Add setter function:
   ```rust
   #[payable]
   #[access_control_any(roles(Role::DAO))]
   pub fn set_new_parameter(&mut self, value: u64) {
       assert_one_yocto();
       let mut config = self.internal_config();
       config.new_parameter = value;
       config.assert_valid();
       self.data_mut().config.set(&config);
   }
   ```

4. Add tests
5. Update documentation

### Adding a New Event

1. Define event in `event.rs`:
   ```rust
   #[near(serializers=[json])]
   pub enum Event {
       // ... existing events
       NewEvent {
           field1: &'a AccountId,
           field2: U128,
       },
   }
   ```

2. Emit after state changes:
   ```rust
   // Perform state changes first
   self.data_mut().value = new_value;

   // Then emit event
   Event::NewEvent {
       field1: &account_id,
       field2: amount,
   }.emit();
   ```

### Adding a New Role-Protected Function

```rust
#[payable]
#[access_control_any(roles(Role::DAO, Role::Operator))]
#[pause(except(roles(Role::DAO)))]
pub fn new_admin_function(&mut self, param: u64) {
    assert_one_yocto();  // Prevent batching

    // Validation
    require!(param > 0, "Invalid parameter");

    // State changes
    self.data_mut().field = param;

    // Emit event
    Event::ConfigChanged { ... }.emit();
}
```

---

## Important Context

### Why Burn Burns from bridge_id

This confused the initial audit. Here's why it's correct:

**NEP-141 Flow:**
1. `ft_transfer` moves tokens from user to bridge
2. `ft_on_transfer` callback lets bridge decide to keep or refund
3. Bridge returns `0` (keep all tokens)
4. Tokens now in bridge balance
5. Later, `burn()` burns from bridge balance (correct!)

The `burn_account_id` parameter is METADATA for events, not the source of burn.

### Why overflow-checks = true Matters

```toml
[profile.release]
overflow-checks = true
```

This means:
- Overflow PANICS even in release mode
- No silent corruption
- Fail-safe behavior
- But still good to use checked_* arithmetic for explicit handling

### NEAR Execution Model

- Callbacks are atomic (no race conditions)
- `#[private]` prevents external calls
- Promise chains don't create reentrancy windows
- Gas is pre-allocated per callback

### Chain Signatures (MPC)

- Private key distributed across nodes
- Threshold signatures (t-of-n)
- No single point of compromise
- Bridge depends on MPC service availability

---

## Files You'll Work With Most

### Core Contracts
- `contracts/satoshi-bridge/src/lib.rs` - Main contract structure
- `contracts/satoshi-bridge/src/api/bridge.rs` - User-facing functions
- `contracts/satoshi-bridge/src/api/management.rs` - Admin functions
- `contracts/satoshi-bridge/src/btc_light_client/deposit.rs` - Deposit logic
- `contracts/satoshi-bridge/src/btc_light_client/withdraw.rs` - Withdraw logic
- `contracts/nbtc/src/lib.rs` - Token contract

### Key Modules
- `contracts/satoshi-bridge/src/nbtc/mint.rs` - Mint callbacks
- `contracts/satoshi-bridge/src/nbtc/burn.rs` - Burn callbacks
- `contracts/satoshi-bridge/src/config.rs` - Configuration
- `contracts/satoshi-bridge/src/event.rs` - Event definitions
- `contracts/satoshi-bridge/src/utxo.rs` - UTXO management

### Don't Modify Without Understanding
- `contracts/satoshi-bridge/src/bitcoin_utils/` - Bitcoin transaction handling
- `contracts/satoshi-bridge/src/psbt.rs` - PSBT validation
- `contracts/satoshi-bridge/src/chain_signature.rs` - MPC signing

---

## Git Workflow

### Main Branches
- `omni-main` - Main branch (use this for PRs)
- `add_utxo_event` - Current working branch

### Commit Message Format
```
<type>: <short description>

<optional longer description>

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
```

Types: feat, fix, refactor, test, docs, chore

### Before Committing
1. Run tests: `cargo test`
2. Check formatting: `cargo fmt`
3. Run clippy: `cargo clippy`
4. Review changes carefully
5. Only commit if user explicitly requested

---

## Common Pitfalls

### ❌ Wrong Assumptions

**DON'T assume:**
- "burn() should burn from user balance" (NO! Already transferred to bridge)
- "overflow without checked_* is silent" (NO! overflow-checks=true causes panic)
- "callbacks can be reentered" (NO! NEAR model prevents this)
- "DAO powers are a bug" (NO! Necessary governance)

### ✅ Correct Understanding

**DO understand:**
- Tokens flow through ft_transfer BEFORE burn
- NEP-141 ft_on_transfer return value controls token disposition
- Callbacks are atomic and #[private]
- Governance is by design, not a vulnerability
- overflow-checks=true protects against silent overflow

### 🔍 When Reviewing Code

**Check for:**
1. Are tokens already transferred before burn/mint?
2. Is callback #[private]?
3. Is access control decorator present?
4. Are events emitted AFTER state changes?
5. Is checked arithmetic used for money operations?
6. Are errors descriptive?

---

## Questions to Ask Before Suggesting Changes

1. **"Where are the tokens right now?"**
   - In user balance? In bridge balance? Not minted yet?

2. **"Is this callback atomic?"**
   - Can it be reentered? (Usually no in NEAR)

3. **"Who can call this function?"**
   - Check decorators: #[private], #[access_control_any]

4. **"What happens if this external call fails?"**
   - Is there error handling? Rollback? Lost & found?

5. **"Is this a bug or a design choice?"**
   - Governance powers, centralization, etc.

6. **"What are the economic incentives?"**
   - Is attack profitable? Cost vs gain?

---

## Useful Commands

### Main Makefile Targets

```bash
# Build all contracts for release (reproducible WASM)
make release

# Build for local development (non-reproducible, faster)
make build-local

# Run linting (fmt + clippy)
make lint

# Run tests (builds local first, then tests both bitcoin and zcash features)
make test
```

### Building Specific Contracts

```bash
# Build nBTC token contract
make nbtc

# Build mock contracts for testing
make mock-dapp
make mock-chain-signatures
make mock-btc-light-client

# Build bridge for specific feature
make build-bitcoin    # Bitcoin bridge
make build-zcash      # Zcash bridge

# Build local (development) for specific feature
make build-local-bitcoin
make build-local-zcash
```

### Testing

```bash
# Run all tests (both bitcoin and zcash features)
make test

# Test specific feature
make test-bitcoin
make test-zcash

# Run tests with output
cargo test --manifest-path contracts/satoshi-bridge/Cargo.toml --features bitcoin -- --nocapture

# Run specific test
cargo test --manifest-path contracts/satoshi-bridge/Cargo.toml --features bitcoin test_name
```

### Code Quality

```bash
# Run formatter
cargo fmt --all

# Check formatting
cargo fmt --all --check

# Run clippy for specific feature
make clippy-bitcoin
make clippy-zcash

# Or directly
cargo clippy --manifest-path contracts/satoshi-bridge/Cargo.toml --features bitcoin
```

### Manual Build Commands (if needed)

```bash
# Build reproducible WASM (for production)
cargo near build reproducible-wasm --manifest-path contracts/satoshi-bridge/Cargo.toml --variant bitcoin

# Build non-reproducible WASM (for development, faster)
cargo near build non-reproducible-wasm --manifest-path contracts/satoshi-bridge/Cargo.toml --features bitcoin --no-abi

# Check contract size
ls -lh res/*.wasm

# Output is in:
# - res/bitcoin_bridge_release.wasm (reproducible)
# - res/bitcoin_bridge.wasm (local dev)
# - res/zcash_bridge_release.wasm (reproducible)
# - res/zcash_bridge.wasm (local dev)
# - res/nbtc.wasm
```

### Common Development Workflow

```bash
# 1. Make changes to code
# 2. Format
cargo fmt --all

# 3. Build locally
make build-local

# 4. Run tests
make test

# 5. Run clippy
make lint

# 6. If all good, build release
make release
```

---

## When in Doubt

1. **Read the full flow** from user action to final state
2. **Check NEP-141 spec** for token standards
3. **Look at existing patterns** in the codebase
4. **Test your assumptions** with actual code traces
5. **Ask the user** if unclear about requirements

---

## Resources

- [NEAR Documentation](https://docs.near.org/)
- [NEP-141 Fungible Token Standard](https://nomicon.io/Standards/Tokens/FungibleToken/Core)
- [near-plugins Documentation](https://github.com/aurora-is-near/near-plugins)
- [Bitcoin Developer Guide](https://developer.bitcoin.org/devguide/)
- [PSBT (BIP 174)](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki)


## Contact

If you encounter security issues or need clarification on architecture:
- Check this CLAUDE.md for context
- Ask the user for clarification

**Remember:** Always understand the full flow before suggesting changes. The BTC bridge is complex, and many "bugs" are actually correct by design.

---

*Last Updated: 2026-02-12*
*Version: 1.0*
