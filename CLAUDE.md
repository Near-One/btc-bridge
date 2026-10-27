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
2. Relayer: bridge.verify_deposit_v2(deposit_msg, tx_bytes, vout, proof)
3. Bridge verifies with Light Client → calls nbtc.mint or nbtc.safe_mint (see Safe Deposits)
4. UTXO added to bridge's available set
```

**Withdraw (nBTC → BTC)**
```
1. User: nbtc.ft_transfer_call(bridge, amount, msg)
   msg = TokenReceiverMessage::Withdraw { target_btc_address, input, output, max_gas_fee, chain_specific_data }
   → Tokens TRANSFERRED to bridge (not burned yet!)
2. nBTC: bridge.ft_on_transfer(user, amount, msg)
   → Bridge validates the BTC tx built from input/output, stores it as pending, returns 0 (keeps tokens)
3. Relayer: bridge.sign_btc_transaction(...) for each input → Chain Signatures signs
4. Tx broadcast to Bitcoin network
5. Relayer: bridge.verify_withdraw_v2(tx_id, proof)
6. Bridge verifies → calls nbtc.burn(user, amount, relayer, fee)
   → Burns from bridge balance (tokens already there!)
```

---

## Security Invariants

### Token Flow (NEP-141)
- **Withdraw tokens already transferred:** By the time `burn()` is called, tokens are in bridge balance via `ft_transfer_call`
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
- **Cross-contract calls are NOT atomic:** Each callback is a separate transaction - must manually rollback state in callback if external call fails

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

## Safe Deposits (Omni Bridge Integration)

`verify_deposit_v2` (and `dao_verify_deposit`) has two flows, selected by `deposit_msg.safe_deposit`:

**Standard (`safe_deposit: None`):**
- Charges deposit bridge fee
- Mints via `nbtc.mint`, which registers the recipient at the token contract's expense
- Mint failure: `mint_callback` does not add the UTXO and removes the verified-deposit marker, so the deposit can be verified again later
- Bitcoin only: Zcash accepts only safe deposits

**Safe (`safe_deposit: Some(SafeDepositMsg)`):**
- Primarily used by Omni Bridge
- NO fees charged
- Caller attaches `required_balance_for_safe_deposit()`; any surplus is refunded
- Recipient must already be registered in nBTC
- `post_actions` are not executed (rejected on Zcash)
- Not atomic: the outcome is decided asynchronously in `safe_mint_callback`

### mint vs safe_mint (nbtc contract)

**mint:** mints to the recipient, registering the account if needed.

**safe_mint:**
- Mints the amount to the bridge account first
- Recipient NOT registered → returns `U128(0)`
- Otherwise transfers to the recipient via `ft_transfer` (returns the amount), or via `ft_transfer_call` if `SafeDepositMsg.msg` is set (returns the amount used by the receiver)

**safe_mint_callback (bridge):**
- Result `0` → burns the minted amount from the bridge, does not add the UTXO, removes the verified-deposit marker (the deposit can be verified again later) and refunds the storage deposit
- Any other result, including a failed or unparsable one → deposit is final and the UTXO is added

---

## Design Decisions (Non-Issues)

These patterns are intentional. Do not flag or "fix" them:

- **DAO powers are by design:** Governance functions with DAO role are necessary, not a vulnerability
- **Expiry height gap:** Buffer for transaction processing delays (Zcash)
- **No validation for self-serialized data:** Format guaranteed by construction - only validate external inputs
- **Public API vs private callbacks:** If parameter cannot be passed through public API, no vulnerability exists

---

## Git Workflow

**Main branch:** `omni-main` (use for PRs)

**Before committing:** Run `cargo test`, `cargo fmt`, `cargo clippy`. **Only commit if user explicitly requests.**

---

*Version: 2.3*
*Last Updated: 2026-10-16*
