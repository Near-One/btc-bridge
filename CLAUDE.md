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

**Contracts:** `contracts/nbtc/` (NEP-141 token), `contracts/satoshi-bridge/` (main bridge), `contracts/mock-*` (testing)

**External Dependencies:** BTC Light Client (Merkle proof verification), Chain Signatures (MPC signing)

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

---

## Security Invariants

### Access Control
- NEVER bypass: `assert_one_yocto()`, `#[private]` callbacks, `assert_bridge()`, `assert_controller()`
- All admin functions must have `#[access_control_any(roles(Role::DAO))]` or similar
- Callbacks must be `#[private]` - no external calls allowed

### Token Flow (NEP-141)
- **Withdraw tokens already transferred:** By the time `burn()` is called, tokens are in bridge balance via `ft_transfer`
- **burn_account_id is for events only:** Actual burn happens from bridge balance, not from burn_account_id
- **ft_on_transfer return value:** `0` = keep all tokens, `amount` = refund amount
- Only burn after BTC tx is verified on-chain

### Arithmetic Safety
- **overflow-checks = true:** All overflow panics in release mode (fail-safe)
- Use `checked_mul()`, `checked_add()` for explicit error handling
- Prefer panic over silent

### State Management
- Mutate state (mark UTXO used, update balances) BEFORE cross-contract calls
- Create and emit events AFTER all state mutations complete
- NEAR execution model: callbacks are atomic, no reentrancy

### Zcash-Specific
- **Mutual exclusion:** `actual_received_amounts.len() == 1` ensures EITHER transparent OR Orchard output, never both
- **OVK required:** All Orchard bundles must provide valid Outgoing Viewing Key for decryption
- **Address restrictions:** Transparent addresses CANNOT accept Orchard bundles (panics)
- **Bridge transparency:** Full transaction tracking required, privacy is NOT a design goal
- **Branch IDs hardcoded:** Network upgrades require contract redeployment anyway

---

## Critical Patterns

**NEAR decorators:** `#[private]` for callbacks, `#[access_control_any(roles(...))]` for admin functions, `#[pause(except(roles(...)))]` for pausable functions, `assert_one_yocto()` to prevent batching

**Security checks:** Always use `require!(condition, "message")` for validation, `checked_*` arithmetic for money operations, emit events AFTER state changes

---

## Safe Functions (Omni Bridge Integration)

The bridge provides "safe" versions of deposit/mint functions primarily used by Omni Bridge:

### verify_deposit vs safe_verify_deposit

**verify_deposit (standard):**
- Normal deposit flow with fees
- Charges deposit bridge fee
- Pays for user's token storage
- Requires `safe_deposit: None` in DepositMsg
- Does NOT revert on mint failures (uses lost & found)

**safe_verify_deposit (integration):**
- Primarily used by Omni Bridge
- NO fees charged
- User must attach NEAR for storage (via `#[payable]`)
- **Reverts entire transaction if mint fails** (no lost & found)
- Requires `safe_deposit: Some(SafeDepositMsg)` in DepositMsg
- **post_actions must be None** (not supported in safe mode)
- Safer for integrations - atomic success/failure

### mint vs safe_mint (nbtc contract)

**mint (standard):**
- Mints tokens unconditionally
- If account not registered → panics or creates account

**safe_mint (integration):**
- Checks if account is registered first
- If NOT registered → returns `U128(0)` instead of panicking
- Used by safe_verify_deposit to detect failures

### Deriving Deposit Addresses

Use `get_user_deposit_address(deposit_msg)` view function:

```rust
// Example DepositMsg for standard deposit
{
  "recipient_id": "user.near",
  "post_actions": null,
  "extra_msg": null,
  "safe_deposit": null  // Must be null for verify_deposit
}

// Example DepositMsg for safe deposit (Omni Bridge)
{
  "recipient_id": "user.near",
  "post_actions": null,
  "extra_msg": null,
  "safe_deposit": {
    "msg": "transfer context for Omni Bridge"
  }
}
```

**CRITICAL:** Derive address BEFORE sending BTC. The deposit address is deterministically generated from DepositMsg hash.

---

## Common Mistakes & Design Decisions

**DON'T assume:**
- burn() should burn from user balance → NO! Already transferred to bridge via ft_transfer
- overflow without checked_* is silent → NO! overflow-checks=true causes panic
- callbacks can be reentered → NO! NEAR model prevents this
- DAO powers are a bug → NO! Necessary governance
- bridge should provide privacy → NO! Transparency is by design

**These patterns are INTENTIONAL (non-issues):**
- Hardcoded branch IDs → Protocol upgrades require redeployment anyway
- Expiry height gap → Buffer for transaction processing delays
- No validation for self-serialized data → Format guaranteed by construction
- Public API vs private callbacks → If parameter can't be passed through public API, no vulnerability

---

## Git Workflow

**Main branch:** `omni-main` (use for PRs)

**Before committing:** Run `cargo test`, `cargo fmt`, `cargo clippy`. **Only commit if user explicitly requests.**

---

## Resources

- [NEAR Documentation](https://docs.near.org/)
- [NEP-141 Fungible Token Standard](https://nomicon.io/Standards/Tokens/FungibleToken/Core)
- [near-plugins Documentation](https://github.com/aurora-is-near/near-plugins)
- [PSBT (BIP 174)](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki)

---

*Version: 2.1*
*Last Updated: 2026-02-16*
