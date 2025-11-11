# Orchard On-Chain Verification Plan (Preflight + Sign Checks)

This document describes how to add on-chain Orchard proof verification to the bridge to avoid "stuck funds" when invalid shielded bundles are submitted. It lays out a phased approach, concrete integration points, testing strategy, and a fork path to embed verifying key/params once gas is measured.

## Problem & Goal

- Problem: Today, a malicious or mistaken submitter can provide invalid Orchard bundle bytes. The Zcash network will only reject this after broadcast. If we reserve inputs and collect signatures before that, user funds can be stuck until timeout/cancel.
- Goal: Add preflight verification on-chain that rejects invalid bundles before entering `PendingSign`. Maintain policy checks (recipient/amount) and digest binding. Optionally verify RedPallas signatures after PCZT is set for extra liveness.

## Current State (as of this branch)

- Sighash includes Orchard bytes so transparent signatures commit to Orchard (anti-malleability):
  - `contracts/satoshi-bridge/src/zcash_utils/psbt_wrapper.rs`
- Policy checks via OVK (enforce recipient & amount):
  - `contracts/satoshi-bridge/src/zcash_utils/orchard_policy.rs`
  - `contracts/satoshi-bridge/src/api/token_receiver.rs`
- Orchard bundle parse and single-action constraint exist; change-only PSBT outputs enforced.
- Example fixture generator to produce UA + Orchard v5 bundle using OVK=00..00:
  - `contracts/satoshi-bridge/examples/gen_orchard_fixture.rs`

## High-Level Design

1) Preflight (before `PendingSign`)
   - Parse bundle, enforce single action and flags; recover output via OVK; enforce recipient/amount.
   - Verify Halo2 Orchard proof on-chain using orchard’s verifier.
   - If any step fails: revert early; do not reserve UTXOs or collect signatures.

2) After PCZT is set (before final sign)
   - Compute ZIP-244 sighash from the frozen transaction (including Orchard).
   - Verify RedPallas SpendAuth signatures (per action) and binding signature over the same sighash.
   - If invalid: revert; prevents signing/broadcast of a doomed tx.

3) Keep digest binding and policy checks as currently implemented.

## Practical Constraints & Strategy

- orchard 0.11.0 + halo2_proofs 0.3.1 expose Params read/write, but there is no public deserializer for `plonk::VerifyingKey`. orchard::circuit::VerifyingKey::build() constructs VK at runtime.
- We have two phases:
  - Phase 1: Runtime keygen on-chain (no fork). Quick path, measure gas with 1–2 actions.
  - Phase 2: Minimal orchard fork to add `VerifyingKey::from_bytes/from_reader` so we can embed VK/params, deserialize, and avoid runtime keygen.

## Phase 1: Runtime VK Build (Feature-Gated)

Implementation steps:

1. Add a feature flag `orchard_proof_verify` in `contracts/satoshi-bridge/Cargo.toml`.

2. Implement `verify_orchard_bundle_preflight(bundle: &orchard::Bundle<Authorized, ZatBalance>) -> Result<(), Error>`:
   - Build VK: `let vk = orchard::circuit::VerifyingKey::build();`
   - Derive instances: `let instances = bundle.actions().iter().map(|a| a.to_instance(*bundle.flags(), *bundle.anchor())).collect::<Vec<_>>();`
   - Verify: `bundle.authorization().proof().verify(&vk, &instances)`.

3. Call preflight verify immediately after bundle parse and OVK recovery (before `PendingSign`):
   - Integration point: `contracts/satoshi-bridge/src/zcash_utils/psbt_wrapper.rs` (in `PsbtWrapper::new` when `orchard_bundle_bytes` is present) or in `zcash_utils/orchard_policy.rs`.
   - Use `#[cfg(feature = "orchard_proof_verify")]` guard; on failure, `require!(false, "Invalid Orchard proof")`.

4. Keep existing policy checks (recipient/amount), structural sanity (action count), and digest binding.

5. Test gas with a 1-action bundle using near-workspaces. If acceptable, enable the feature by default.

Status and measurements (as implemented):

- Implemented behind `orchard_proof_verify` and enabled by default.
- Gas (1-action, includes VerifyingKey::build + proof verify): ~11.7 Tgas.
- Size impact on satoshi-bridge Wasm (cargo-near optimized):
  - zcash only: ~1.28 MB (`res/zcash.wasm`)
  - zcash + inline proof verify: ~2.07 MB (`res/zcash_verify.wasm`)
  - Delta ≈ 0.79 MB (Halo2 + Orchard circuit verifier stack)
  - Note: a plain cargo build was ~3.54 MB before wasm-opt; always compare cargo-near outputs.

Note: The “keygen” here is only VK build for a fixed circuit; PK/proof creation is never done on-chain.

## Phase 2: Embedded VK/Params (Minimal Orchard Fork)

1. Fork `orchard` v0.11.0 (pin to a commit or tag). Minimal changes in `src/circuit.rs`:
   - Add `impl VerifyingKey { pub fn from_reader<R: Read>(mut rp: R, mut rvk: R) -> Result<Self, io::Error> }` or `from_bytes(params: &[u8], vk: &[u8])`.
   - Internally, use halo2_proofs `Params::<vesta::Affine>::read` for params and provide a deserializer for `plonk::VerifyingKey<vesta::Affine>` (you may need to add read to halo2 or reconstruct via a pinned representation if available; simplest is exposing orchard’s internal serialization used during keygen).

2. Write a small generator tool (repo `tools/` or `examples/`) that:
   - Builds VK once via `VerifyingKey::build()`.
   - Serializes params & VK to bytes.
   - Emits a Rust module (e.g. `orchard_vk_bytes.rs`) with `include_bytes!` constants.

3. Add a loader module in the contract (e.g. `zcash_utils/orchard_vk_loader.rs`) that deserializes the bytes into a `VerifyingKey` using the forked API.

4. Replace Phase 1’s runtime `build()` with `from_bytes` deserialization to avoid per-call keygen cost. Measure gas again.

Expected effect: compute savings vs VK build; binary size remains similar because verifier code is still linked.

## Phase 1.5: Verifier Contract (Cross-Contract Call)

Motivation: keep the bridge Wasm small (deployable under near-workspaces’ ~1.5 MB tx cap) while paying the proof-verify gas in a dedicated contract.

Design:

- Add `orchard_verifier_account_id: Option<AccountId>` to Config (zcash feature).
- In `ft_on_transfer_callback`, if an Orchard bundle is present and the verifier is configured:
  - Call `verifier.verify_orchard_bundle(bundle_hex)` and return.
  - In `orchard_verify_callback`, `require!(is_promise_success())`, then build the PSBT and advance to `PendingSign`.
- Skip inline verification when an external verifier is configured to avoid double work.

Measured gas (minimal verifier): ~11.7 Tgas for a 1-action bundle (VK build + verify).

## RedPallas Signature Verification (Optional, Recommended)

Add a feature `orchard_sig_verify` and implement in `create_btc_pending_info` after `set_input_utxo`:

1. Compute ZIP-244 `txid_parts` from the frozen transaction (already implemented) and the signable input.
2. For each Orchard action, verify RedPallas `SpendAuth` signature using the `rk` from the action over the computed sighash.
3. Verify the binding signature using the bundle’s `cv_net` (or orchard helper if available).
4. Fail early if any signature invalid.

## Integration Points

- Preflight verify:
  - `contracts/satoshi-bridge/src/zcash_utils/psbt_wrapper.rs` (after `read_v5_bundle` + OVK checks).
  - Consider placing verification logic in `zcash_utils/orchard_policy.rs` for coherence.
  - Cross-contract path: `contracts/satoshi-bridge/src/zcash_utils/contract_methods.rs` implements `orchard_verify_callback` and offloading logic.

- RedPallas verify (after PCZT):
  - `contracts/satoshi-bridge/src/api/token_receiver.rs` inside `create_btc_pending_info` once inputs are set.

- Existing digest binding and policy checks remain unchanged.

## Testing Strategy

1. Unit tests (no sandbox):
   - Build a valid bundle using `orchard::builder` (single output with OVK=00..00); call preflight verify and assert OK.
   - Corrupt proof bytes; assert preflight fails.

2. Fixture generator (already present):
   - `examples/gen_orchard_fixture.rs` produces UA + bundle hex; useful for manual runs and sanity checks.

3. near-workspaces test:
   - Use `near_workspaces::compile_project` to auto-build WASMs.
   - Minimal verifier gas test: compiles and deploys a small contract performing VK build + verify and prints `total_gas_burnt`.
   - Full flow: prefer the cross-contract verifier to keep the bridge deployable, then seed UTXO, build PSBT (change-only), call withdraw with bundle, and assert state transitions gated by `orchard_verify_callback`.

## Gas & Size Considerations

- Phase 1 (runtime keygen) is compute-heavy but easy to ship and measure.
- Phase 2 (embedded VK/params) trades larger Wasm for much lower per-call compute.
- Keep bundles to a single action initially to bound costs.

Current baselines (cargo-near optimized):
- zcash only bridge: ~1.28 MB
- zcash + inline verify: ~2.07 MB (exceeds near-workspaces 1.5 MB cap)
- Minimal verifier: ~1.09 MB; per-call verify ~11.7 Tgas (single action)

## Security & Privacy Notes

- OVK-based recovery reveals recipients/amounts for outputs created with that OVK. This only affects bridge-created outputs. Consider adding a DAO-configurable OVK to allow rotation.
- On-chain proof & signature verify eliminate relayer trust for liveness; the network later reconfirms what we preflighted.

## Task Breakdown for Next Implementation Pass

1) Phase 1: Add `orchard_proof_verify` flag and preflight verify call (runtime `VerifyingKey::build()`), fail-fast on invalid proofs.
2) Add `orchard_sig_verify` and verify RedPallas SpendAuth + binding signatures after PCZT is set.
3) Add near-workspaces test that uses the existing generator (env-driven) or inline generation; optionally add a test-only UTXO seeding method guarded by `test-utils` and dev_deploy a test wasm.
4) Measure gas/headroom.
5) Phase 2: Create a minimal orchard fork to add `VerifyingKey::from_bytes` and embed/deserialize VK/params in the contract; switch preflight to use deserialized VK.
6) Optional: Add DAO-configurable OVK and feature flags for toggling enforcement.
