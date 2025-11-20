# Orchard Test Suite Guide

Quick reference for running and understanding the Zcash Orchard bundle tests.

## Running the Tests

### ⚠️ IMPORTANT: Build the contract first!

Before running tests, you MUST build the contract with Zcash support:

```bash
# From the satoshi-bridge directory
cd contracts/satoshi-bridge
cargo near build non-reproducible-wasm --features zcash --out-dir ../../res --no-abi
```

**Why?**
- Tests deploy the WASM from `res/satoshi_bridge.wasm`
- If you don't rebuild after code changes, tests will use stale code!
- The optimized build (~1.3MB) fits under NEAR's 1.57MB limit

**Note:** Use `--no-abi` because ABI generation fails with bitcoin types (missing JsonSchema).

---

### Run all Orchard tests
```bash
cargo test -p satoshi-bridge --features zcash --test test_orchard_validation --test test_orchard_withdrawal -- --test-threads=1 --nocapture
```

### Run individual test files
```bash
# Validation tests
cargo test -p satoshi-bridge --features zcash --test test_orchard_validation -- --test-threads=1

# Withdrawal tests
cargo test -p satoshi-bridge --features zcash --test test_orchard_withdrawal -- --test-threads=1
```

### Run specific test
```bash
cargo test -p satoshi-bridge --features zcash test_orchard_wrong_recipient -- --test-threads=1 --nocapture
```

**Note:** Tests MUST run with `--test-threads=1` because they share sandbox environments and cached bundles.

---

## test_orchard_validation.rs

Tests edge cases and validation logic for Orchard bundles.

### ✅ test_orchard_wrong_recipient
**What it tests:** Security check that prevents using a bundle intended for recipient A while claiming it's for recipient B.

**How it works:**
1. Generates two bundles with different spending keys → different recipients
2. Uses bundle A but claims target_btc_address is recipient B
3. Verifies contract rejects with "Orchard recipient mismatch"

**Why it matters:** Critical security boundary - prevents malicious users from stealing bundles.

**Runtime:** ~2-3 minutes (generates 2 Orchard bundles with proofs)

---

### ✅ test_orchard_missing_bundle
**What it tests:** Validates that providing a Unified Address without an Orchard bundle is rejected.

**How it works:**
1. Generates a Unified Address (u1...)
2. Attempts withdrawal with UA but `orchard_bundle_bytes: None`
3. Verifies contract rejects with helpful error message

**Why it matters:** Prevents ambiguous behavior and user errors. Clear contract semantics.

**Runtime:** ~10-15 seconds

---

### ✅ test_orchard_bundle_in_zcash_tx
**What it tests:** End-to-end flow that the signed Zcash transaction includes the Orchard bundle.

**How it works:**
1. Deposit → Withdraw with Orchard bundle
2. Sign the transaction
3. Verifies the final transaction bytes contain the Orchard data

**Why it matters:** Ensures the full withdrawal flow works and bundles are properly serialized.

**Runtime:** ~20 seconds

---

## test_orchard_withdrawal.rs

Tests complete withdrawal flows with Orchard bundles.

### ✅ test_orchard_withdrawal_with_ovk_validation
**What it tests:** Main e2e test - complete Orchard withdrawal with OVK (Outgoing Viewing Key) recovery.

**How it works:**
1. Deposits funds for Alice using Zcash v5 transaction
2. Alice withdraws to a Unified Address with Orchard bundle
3. Signs the withdrawal transaction
4. Verifies the transaction using OVK to recover recipient and amount
5. Validates correct fees were deducted

**Why it matters:** This is the primary happy-path test that validates the entire Orchard feature works end-to-end.

**Runtime:** ~15 seconds

---

### ✅ test_orchard_withdrawal_amount_mismatch
**What it tests:** Validates that providing a bundle with incorrect amount is rejected.

**How it works:**
1. Calculates expected Orchard amount after fees: `withdraw_amount - withdraw_fee - gas_fee`
2. Generates bundle for 100,000 zatoshis (wrong amount)
3. Attempts withdrawal that expects 370,000 zatoshis
4. Verifies contract detects the mismatch

**Why it matters:** Ensures users can't cheat by providing bundles with incorrect amounts.

**Runtime:** ~1-2 minutes (generates Orchard bundle with proof)

---

## Design Decisions

### Single-Action Only
All tests use **single-action Orchard bundles** (`BundleType::Coinbase`). Multi-action support is intentionally NOT implemented because:
- Bridge is a 1:1 operation (burn X → receive X)
- Simpler validation = easier to audit
- Bridge operator already knows amounts/recipients
- Can be added later if needed

Validation enforced at: `psbt_wrapper.rs:92-95`

### Bundle Caching
Orchard bundle generation is expensive (~80-90s per bundle due to Halo2 proof). Tests cache generated bundles for reuse:

**Cache file patterns:**
- Default key: `tests/orchard_bundle_cache_{amount}.txt`
- Custom key: `tests/orchard_bundle_cache_{amount}_{spending_key_hex}.txt`

**Performance:**
- First run: ~175s (generates 2 bundles with proofs)
- Subsequent runs: ~16s (loads from cache) ⚡

**To regenerate caches:**
```bash
rm contracts/satoshi-bridge/tests/orchard_bundle_cache_*.txt
```

---

## Test Summary

| Test | File | Status | Runtime | Purpose |
|------|------|--------|---------|---------|
| test_orchard_withdrawal_with_ovk_validation | test_orchard_withdrawal.rs | ✅ Pass | 15s | Main e2e happy path |
| test_orchard_withdrawal_amount_mismatch | test_orchard_withdrawal.rs | ✅ Pass | 90s | Amount validation |
| test_orchard_wrong_recipient | test_orchard_validation.rs | ✅ Pass | 160s | Recipient security |
| test_orchard_missing_bundle | test_orchard_validation.rs | ✅ Pass | 13s | UA validation |
| test_orchard_bundle_in_zcash_tx | test_orchard_validation.rs | ✅ Pass | 20s | Serialization check |

**Total:** 5 tests, all passing ✅
