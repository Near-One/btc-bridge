# NEAR BTC/Zcash Bridge

Bridge between Bitcoin/Zcash and NEAR Protocol. Users deposit BTC/ZEC to receive nBTC/nZEC (NEP-141 token) and withdraw nBTC/nZEC to receive BTC/ZEC back.

**Trust Model:**
- **BTC → NEAR (deposit):** Trustless verification via BTC Light Client (Merkle proof validation)
- **NEAR → BTC (withdraw):** Requires trust in NEAR validator set for Chain Signatures (MPC)

---

## Build / Test / Lint

```bash
# Build for development (non-reproducible)
make build-local-bitcoin    # Bitcoin bridge
make build-local-zcash      # Zcash bridge

# Build for release (reproducible)
make release

# Run tests
make test

# Format and clippy
cargo fmt --all             # Format all code
make clippy-bitcoin         # Clippy for Bitcoin
make clippy-zcash           # Clippy for Zcash
```

---

## Key Architecture

### Contracts
- **contracts/nbtc/** - NEP-141 fungible token (nBTC)
- **contracts/satoshi-bridge/** - Main bridge managing deposits/withdrawals/UTXOs
- **contracts/mock-*** - Testing utilities

### External Dependencies
- **BTC Light Client** - SPV verification via Merkle proofs
- **Chain Signatures (MPC)** - Distributed key for Bitcoin signing

### Bridge Flows

**Deposit (BTC → nBTC)**
```
1. User sends BTC to deposit address (derived from DepositMsg hash)
2. Relayer: bridge.verify_deposit(tx_proof)
3. Bridge verifies with Light Client → calls nbtc.mint(user, amount)
4. UTXO added to bridge's available set
```

**Withdraw (nBTC → BTC)**
```
1. User: nbtc.ft_transfer(bridge, amount, WithdrawMsg)
   → Tokens TRANSFERRED to bridge (not burned yet!)
2. nBTC: bridge.ft_on_transfer(user, amount, msg) → Bridge returns 0 (keeps tokens)
3. Bridge creates BTC tx, Chain Signatures signs
4. Tx broadcast to Bitcoin network
5. Relayer: bridge.verify_withdraw(tx_proof)
6. Bridge verifies → calls nbtc.burn(user, amount, relayer, fee)
   → Burns from bridge balance (tokens already there!)
```

**CRITICAL:** `burn_account_id` parameter is for ACCOUNTING/EVENTS only. Actual burn happens from `bridge_id` balance where tokens already are after `ft_transfer`. This is NEP-141 standard behavior.

---

## Zcash Orchard Support

Bridge supports both transparent (Bitcoin-style) and Orchard (shielded) outputs:

- **Mutual Exclusion:** `actual_received_amounts.len() == 1` ensures EITHER transparent OR Orchard output, never both
- **OVK Validation:** Orchard outputs require Outgoing Viewing Key to decrypt and verify recipient
- **Address Restrictions:** Transparent addresses CANNOT accept Orchard bundles (panics on `extract_orchard_receiver()`)
- **Bridge Transparency:** Bridge operates with full transparency, privacy is NOT a design goal

---

## Security Invariants

### Access Control
- **NEVER bypass:** `assert_one_yocto()`, `#[private]` callbacks, `assert_bridge()`, `assert_controller()`
- **All admin functions:** Must have `#[access_control_any(roles(Role::DAO))]` or similar
- **Callbacks:** Must be `#[private]` - no external calls allowed

### Token Flow
- **Withdraw tokens already transferred:** By the time `burn()` is called, tokens are already in bridge balance via `ft_transfer`
- **NEP-141 ft_on_transfer:** Bridge returns `0` = keep all tokens, `amount` = refund amount
- **No burn without verification:** Only burn after BTC tx is verified on-chain

### Arithmetic Safety
- **overflow-checks = true:** All overflow panics in release mode (fail-safe)
- **Use checked_*:** For explicit error handling: `checked_mul()`, `checked_add()`
- **Never silent corruption:** Prefer panic over wrong amounts

### State Management
- **State before external calls:** Mutate state (mark UTXO used, update balances) BEFORE cross-contract calls
- **Events after state changes:** Create and emit events AFTER all state mutations complete
- **Atomic callbacks:** NEAR execution model prevents reentrancy, callbacks are atomic

### Zcash Validation
- **Orchard mutual exclusion:** Check `actual_received_amounts.len() == 1` prevents mixed outputs
- **OVK required:** All Orchard bundles must provide valid OVK for decryption
- **Change < dust is valid:** Transparent change CAN be less than dust (546 sats) in Zcash - this is intentional
- **Branch IDs hardcoded:** Network upgrades require contract redeployment anyway

---

## Critical Patterns

### NEAR-Specific
```rust
// Callbacks must be #[private]
#[private]
pub fn callback_after_external_call(...) { }

// Access control decorators
#[access_control_any(roles(Role::DAO, Role::Operator))]
pub fn admin_function(...) { }

// Pausable with exceptions
#[pause(except(roles(Role::DAO)))]
pub fn user_function(...) { }

// Prevent batching
#[payable]
pub fn sensitive_operation(...) {
    assert_one_yocto();
    // ...
}
```

### Security Checks
```rust
// Always validate input
require!(condition, "Clear error message");

// Checked arithmetic for money
amount.checked_mul(rate)
    .unwrap_or_else(|| env::panic_str("overflow"));

// Events after state changes
self.internal_set_utxo(&key, utxo);
Event::UtxoAdded { utxo_storage_keys: vec![key] }.emit();
```

---

## Key Files

### Core Contracts
- `contracts/satoshi-bridge/src/lib.rs` - Main contract
- `contracts/satoshi-bridge/src/api/bridge.rs` - User-facing functions
- `contracts/satoshi-bridge/src/api/management.rs` - Admin functions
- `contracts/satoshi-bridge/src/btc_light_client/` - Deposit/withdraw verification
- `contracts/nbtc/src/lib.rs` - Token contract

### Critical Modules
- `contracts/satoshi-bridge/src/psbt.rs` - PSBT validation (DON'T modify without deep understanding)
- `contracts/satoshi-bridge/src/zcash_utils/orchard_policy.rs` - Orchard bundle validation
- `contracts/satoshi-bridge/src/config.rs` - Configuration and validation

---

## Git Workflow

**Main Branch:** `omni-main` (use for PRs)

**Before Committing:**
1. Run tests: `cargo test`
2. Format: `cargo fmt`
3. Clippy: `cargo clippy`
4. **Only commit if user explicitly requests**

**NEVER:**
- Push to remote without explicit request
- Force push to main/omni-main
- Use `--no-verify`
- Commit without user asking

---

## Common Pitfalls

**DON'T assume:**
- "burn() should burn from user balance" → NO! Already transferred to bridge
- "overflow without checked_* is silent" → NO! overflow-checks=true causes panic
- "callbacks can be reentered" → NO! NEAR model prevents this
- "DAO powers are a bug" → NO! Necessary governance
- "transparent change must be >= dust" → NO! In Zcash < dust is valid

**DO understand:**
- Tokens flow: user → bridge (via ft_transfer) → burn from bridge balance
- NEP-141 ft_on_transfer return value controls token disposition
- Callbacks are atomic and #[private]
- `actual_received_amounts.len() == 1` prevents attack vectors
- Bridge transparency is by design, not a limitation

---

## Design Decisions (Non-Issues)

These patterns are intentional. Do not flag or "fix" them:

- **Burn from bridge balance:** Tokens already transferred via `ft_transfer` before burn
- **Bridge has no privacy:** Full transaction tracking required for validation
- **Hardcoded branch IDs:** Protocol upgrades require redeployment anyway
- **Expiry height gap:** Buffer for transaction processing delays
- **No validation for self-serialized data:** Format guaranteed by construction
- **Transparent change < dust allowed:** Valid in Zcash protocol
- **Public API vs private callbacks:** If parameter can't be passed through public API, no vulnerability

---

## When Modifying Contracts

**Always ask yourself:**
1. Where are the tokens right now? (user balance? bridge balance? not minted?)
2. Is callback #[private]? Can external contracts call it?
3. Is access control decorator present?
4. Are events emitted AFTER state changes?
5. Is this a bug or a design choice?

**Before suggesting changes:**
- Read the full flow from user action to final state
- Check if tokens are already transferred
- Verify callback execution order
- Consider NEAR execution model (atomic callbacks)
- Look at existing patterns in the codebase

---

## Resources

- [NEAR Documentation](https://docs.near.org/)
- [NEP-141 Fungible Token Standard](https://nomicon.io/Standards/Tokens/FungibleToken/Core)
- [near-plugins Documentation](https://github.com/aurora-is-near/near-plugins)
- [PSBT (BIP 174)](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki)

**Remember:** Always understand the full flow before suggesting changes. Many "bugs" are actually correct by design.

---

*Version: 2.0*
*Last Updated: 2026-02-16*
