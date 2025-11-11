# Orchard Bundle Verification in btc-bridge

This document consolidates design goals, constraints, options, and a concrete plan for handling Orchard bundle verification in the bridge. It draws on the current codebase, PR #6 reviews, and Issue #16 context.

## Summary

- Verifying Orchard strictly from serialized bundle bytes is not possible. Correct verification depends on the full ZIP-244 sighash, which requires PCZT (transparent prevout scripts and amounts).
- We can avoid on-chain Halo2 proof verification by using low-gas checks that are sufficient for our threat model:
  1) Bind transparent signatures to the exact Orchard bytes (include the bundle in ZIP-244 txid parts).
  2) Recover the output with a known OVK and check recipient and amount (policy check).
  3) Optional: Verify RedPallas SpendAuth and binding signatures on-chain once PCZT is set (cheaper than Halo2).
- If we still want full on-chain proof verification, precompute and embed Halo2 params/VK, then reconstruct at runtime (requires a tiny orchard fork to expose deserialization). This adds Wasm size and maintenance overhead.

## Current Code Paths

- Parse Orchard bundle (feature-gated; enabled by default):
  - `contracts/satoshi-bridge/src/zcash_utils/psbt_wrapper.rs:71`
- Build TransactionData for sighash (currently omits Orchard bundle):
  - `contracts/satoshi-bridge/src/zcash_utils/transaction.rs:98`
- API paths that thread `orchard_bundle`:
  - `contracts/satoshi-bridge/src/api/bridge.rs:214`
  - `contracts/satoshi-bridge/src/zcash_utils/contract_methods.rs:27`

Relevant reviews:
- PR #6 (“feat: shielded TX support”) comments: You can’t verify Orchard from bytes only; need PCZT.
- Issue #16: Example code from karim for OVK-based recovery + proof verify.

## Key Concepts

- ZIP-244 sighash (NU5): The per-transaction digest that all component signatures (transparent, Sapling, Orchard) bind to. It depends on the transparent prevouts (PCZT), so it cannot be computed from the serialized transaction alone.
- PCZT: The set of transparent prevout scripts and amounts for each input. Required to compute ZIP-244 sighash and to verify component signatures.
- Orchard bundle: A group of actions with a Halo2 proof, SpendAuth signatures (per action), and a binding signature (for value balance). Actions include nullifiers, commitments, cv_net, and encrypted note data.
- RedPallas: The signature scheme used in Orchard (a RedDSA variant over the Pallas curve). Two kinds are used:
  - SpendAuth signatures: Per-action signatures using randomized validating keys `rk`.
  - Binding signature: A transaction-level signature tied to the net value commitment `cv_net` to prevent value-balance malleability.

### Understanding PCZT and Daira’s Comment

Comment recap: “You can't verify that just based on the bytes of the Orchard section of the transaction. You need to have the data that was encrypted and encoded to produce those bytes. That is available in the PCZT.”

What this means:
- The Orchard bundle contains commitments and ciphertexts (e.g., `out_ciphertext`), plus proof and signatures. From these bytes alone, you cannot prove that “the output is to recipient X for amount Y,” because the recipient and amount are encrypted.
- The “data that was encrypted and encoded” refers to the note plaintext and randomness used to produce the ciphertext and commitments: recipient key material (g_d, pk_d), note value, rseed-derived randomness (rcm/psi), value commitment trapdoor (rcv), etc. Verifying that the ciphertext and commitments match specific plaintext requires those values.
- PCZT (supported by `orchard::pczt`) is a structured representation that carries the necessary per-action data (spend/output values, randomness, keys, etc.) to re-derive and verify what was committed/encrypted. It can also facilitate finalizing signatures against the full ZIP‑244 sighash.

Do we have those bytes on-chain?
- In this repo’s current design, the contract only receives the serialized Orchard bundle bytes (`Option<Vec<u8>>`). It does NOT receive PCZT. Therefore, on-chain we cannot derive or confirm the plaintext behind the ciphertext, nor can we verify Orchard SpendAuth/binding signatures (they require the ZIP‑244 sighash which depends on PCZT for transparent prevouts).
- Workarounds when PCZT is not available on-chain:
  - Use a known OVK (hardcoded constant) to recover outputs and compare recipient/amount. This substitutes a viewing key for the missing plaintext, and is practical for enforcing bridge policy.
  - Bind transparent signatures to the Orchard bytes (include Orchard in the ZIP‑244 sighash) so any post‑hoc mutation of the Orchard section breaks signatures.
  - Perform full verification off-chain (relayer) and optionally attest on-chain.

## What Can Be Verified From Orchard Bytes Alone

- You can parse the bundle, inspect action counts, flags, sizes, and compute Orchard commitments.
- You cannot verify SpendAuth/binding signatures nor bind the bundle to a specific transaction without PCZT (needed for ZIP-244 sighash).
- You cannot confirm recipient/amount from ciphertext without a viewing key; recovery requires the sender or a known OVK.

## Options and Tradeoffs

1) Bind Orchard into the sighash (RECOMMENDED – do now)
- Include the Orchard bundle in `TransactionData` used for `get_hash_to_sign()`, so any change to the bundle invalidates transparent signatures collected by the contract.
- Gas: minimal; Complexity: low.

2) OVK-based output recovery (RECOMMENDED – do now)
- Hardcode a 32-byte OVK constant; require builders to use it. Recover the output and check recipient and amount against bridge expectations.
- Gas: minimal; Complexity: low; Privacy: the bridge can recover outputs made with that OVK.

3) Verify RedPallas SpendAuth + binding signatures (OPTIONAL – strong, still cheap)
- After PCZT is set in `PsbtWrapper`, compute the ZIP-244 sighash and verify:
  - Each action’s SpendAuth signature using its `rk` over the same sighash.
  - The bundle binding signature using a key derived from `cv_net`.
- Gas: moderate; Complexity: medium; No Halo2 verification required.

4) Verify Halo2 Orchard proof on-chain (NOT NEEDED; OPTIONAL)
- Runtime keygen via `VerifyingKey::build()` is heavy in Wasm (FFT/MSM), and cannot be cached across calls.
- Embedding params/VK avoids keygen but increases Wasm size; needs a small orchard fork to expose deserialization.

5) Relayer quorum attestation (OPTIONAL)
- Require m-of-n relayers to sign a statement that Orchard verifies for the specific PCZT and bundle. Cheap on-chain, aligns with trust you already place in relayers for co-signing.

## Halo2 Proof Verification: What It Does (and Doesn’t)

What it is:
- Calling `bundle.verify_proof(&VerifyingKey)` checks the Orchard Action circuit’s zk‑proof using public inputs extracted from the bundle (`anchor`, `cv_net`, `nf_old`, `rk`, `cmx`, enable flags).

What it guarantees:
- Internal consistency of the Orchard section: correct Merkle path to `anchor`, correct commitments and value arithmetic, and that the declared `cv_net` and flags are consistent with those commitments.

What it does not guarantee:
- It does not bind the bundle to your specific transaction. Binding to the full ZIP‑244 sighash (which depends on PCZT) comes from RedPallas SpendAuth and binding signatures, verified over that sighash.
- It does not reveal or enforce recipient/amount (those are encrypted). You need a viewing capability (e.g., OVK) or plaintext via PCZT to check policy.
- It does not replace network validation. Zcash nodes re‑verify both the proof and signatures on inclusion.

Relevance to this bridge:
- The contract only burns nBTC after on‑chain inclusion, meaning the network has already validated the Orchard proof/signatures. On-chain proof verification adds little safety at that point and consumes gas.
- Pre‑inclusion, on-chain proof verification would merely pre‑filter invalid bundles that off‑chain builders/relayers should already reject.

## Recommended Approach

- Implement 1) and 2) immediately.
- Optionally add 3) for stronger binding without Halo2 cost.
- Keep 4) behind a cargo feature for benchmarking only; disabled in production unless it’s affordable.

## Implementation Details

### Bind Orchard Into ZIP-244 Sighash

- Location: `contracts/satoshi-bridge/src/zcash_utils/transaction.rs:82`
- Today we build `TransactionData` with `None` for the Orchard bundle; update it to include an effects-only Orchard bundle:
  - Convert `Bundle<Authorized, V>` → `Bundle<EffectsOnly, V>` using `map_authorization` (erase signatures/proof), then pass `Some(orchard_effects_bundle)` into `TransactionData::from_parts`.
- Result: `get_hash_to_sign()` produces `txid_parts` that commit to the Orchard bytes.

### OVK Recovery and Policy Validation

- Define an OVK constant (configurable if desired), e.g.:
  - `const BRIDGE_OVK: [u8; 32] = [0u8; 32];`
- At bundle parse time (`psbt_wrapper.rs:71`):
  - Parse with `read_v5_bundle` and unwrap.
  - Enforce `actions().len() == 1` initially.
  - Recover output: `bundle.recover_output_with_ovk(0, &orchard::keys::OutgoingViewingKey::from(BRIDGE_OVK))`.
  - Compare recovered `note.value()` and `addr` to the expected amount and address from the bridge context.
- Thread `expected_amount` and `expected_recipient` to `PsbtWrapper::new(...)` or derive them from the higher-layer call.

#### Privacy implications of a known OVK
- Using a fixed, known OVK allows anyone who knows that OVK to recover outputs created with it. Because contract code is public, a hardcoded OVK is effectively public. This reveals recipient and amount for bridge‑created shielded outputs. It does not deanonymize unrelated Zcash activity.
- If this trade‑off is unacceptable, prefer RedPallas verification + signature binding and rely on network inclusion, or use off‑chain relayer attestations.

### RedPallas Signature Verification (Optional)

Purpose: Tie Orchard to the same ZIP-244 sighash (with PCZT) without running Halo2.

High-level steps:
- Ensure `TransactionData` includes the Orchard bundle (effects-only) and the transparent inputs with their prevout scripts/amounts set via `set_input_utxo`.
- Compute `txid_parts` and the ZIP-244 signature hash for the Orchard component. Use the same sighash function as used for transparent signing (already in use).
- For each action in the bundle:
  - Extract `rk` (randomized validating key) and `spend_auth_sig`.
  - Verify RedPallas `spend_auth_sig` with `rk` over the ZIP-244 sighash.
- For the bundle binding signature:
  - Compute the binding verification key from `cv_net` or use the orchard helper to verify the binding signature over the same sighash.

Notes:
- The orchard crate exposes RedPallas signatures and keys via `orchard::primitives::redpallas`. You verify over a prehashed message (the ZIP-244 sighash).
- This check is significantly cheaper than Halo2 proof verification and detects tampering that changes the transaction contents.

### Halo2 Proof Verification (Feature-Gated)

- Enable orchard `circuit` feature in Cargo for access to the verifier.
- Under a cargo feature (e.g., `orchard_proof_verify`), call:
  - `let vk = orchard::circuit::VerifyingKey::build();`
  - `bundle.verify_proof(&vk).unwrap();`
- Only enable for benchmarking; disable in production unless gas is acceptable.

### Why We Are Not Hardcoding Halo2 VK/Params (for now)

Hardcoding the Orchard verifier parameters and verifying key is feasible, but we’re not choosing it now for these reasons:

- Limited security gain for this bridge:
  - Network inclusion already re‑verifies the Orchard proof and signatures before we burn nBTC.
  - Halo2 proof verification alone does not bind the bundle to our transaction (PCZT) or enforce recipient/amount; the highest‑leverage mitigations are sighash binding, RedPallas checks, and policy enforcement (OVK or relayer attestations).

- Non‑trivial engineering overhead:
  - Orchard does not expose a stable `from_bytes` for `VerifyingKey`/params; we would fork orchard to add deserialization or expose fields.
  - We need a generator tool to produce the bytes, a loader in the contract, and tests to ensure the embedded bytes match the pinned versions.

- Maintenance risk and version pinning:
  - Any upgrade of `orchard`/`halo2_proofs` or a change in circuit size (K) invalidates the embedded bytes; we must regenerate and retest.
  - We’d also need CI checks to catch drift (e.g., verify a sample bundle with both runtime and embedded VKs).

- Wasm size and per‑call cost:
  - Embedding params/VK increases Wasm size substantially (tens–hundreds of KB), pushing limits and increasing cold‑start costs.
  - NEAR does not persist memory across calls, so we still pay per‑call deserialization costs; it’s cheaper than keygen, but not free.

- Simpler alternatives exist:
  - Sighash binding + optional RedPallas checks are cheaper and directly address tampering.
  - OVK‑based policy checks (or relayer attestations) enforce recipient/amount without shipping large constants.

We may revisit this choice if we later need on‑chain proof verification for a strong pre‑inclusion filter and can justify the size/gas trade‑off. For now, we prioritize the mitigations with the best security‑per‑gas returns.

## Where Orchard Bytes Come From

- The off‑chain builder/relayer supplies serialized Orchard bundle bytes to contract calls that construct PSBTs (e.g., `ft_on_transfer` and active UTXO management). They are parsed in `psbt_wrapper`.
- The contract does not receive PCZT or the Orchard plaintext; it only gets the serialized bundle bytes.

## Can “Bad” Orchard Bytes Drain the Bridge?

- No, not by themselves:
  - Invalid bundles (bad proofs/signatures) are rejected by Zcash nodes; your contract burns nBTC only after on‑chain inclusion is proven.
  - Binding Orchard bytes into the ZIP‑244 sighash used for transparent inputs prevents post‑signing swaps of the Orchard section.
- The practical risk is policy mismatch (a valid bundle that pays the wrong recipient/amount). Mitigations:
  - OVK recovery to enforce recipient/amount on-chain; or
  - Off‑chain relayer verification (and optional on‑chain attestation); or
  - On‑chain RedPallas signature checks after PCZT is set (binds Orchard to the exact transaction without revealing recipient).

### Hardcoding VK/Params (If Required)

- Fork orchard minimally to add `VerifyingKey::from_reader` and params deserialization.
- Build a small tool to generate params/VK bytes from `VerifyingKey::build()` using the exact pinned versions and K.
- Embed with `include_bytes!` and reconstruct on-chain.
- Pros: avoid keygen; Cons: larger Wasm, added maintenance when versions change.

## Security Considerations

- Network inclusion is authoritative: If your final verification step is checking block inclusion via Merkle proof, the Zcash network has already validated the Orchard proof and signatures.
- Sighash binding prevents malleability: Transparent signatures will fail if Orchard bytes are changed.
- OVK recovery enforces policy: Ensures funds go to the intended recipient with the expected amount.
- RedPallas verification further ties Orchard to PCZT without Halo2 costs.

## Gas and Performance

- Sighash binding and OVK recovery are negligible in gas.
- RedPallas signature verification is moderate but feasible in Wasm.
- Halo2 proof verification:
  - Runtime keygen is expensive. Avoid in production.
  - With embedded VK/params, verification cost is lower but not trivial; also increases Wasm size and per-call deserialization cost.

## Maintenance and Testing

- If embedding VK/params, pin orchard and halo2 versions; regenerate bytes on upgrade.
- Add near-workspaces tests to measure gas for:
  - OVK recovery + sighash binding.
  - Optional RedPallas verification.
  - Optional Halo2 verification (feature-gated) for benchmarking only.
- Add structural sanity checks on bundle size/action count to mitigate OOG vectors.

## Open Decisions

- BRIDGE_OVK value and whether to make it DAO-configurable.
- Enforce single action vs. support multiple actions in the future.
- Whether to add on-chain RedPallas verification now or later.
- Remove `orchard_bundle` from active UTXO management APIs per PR feedback.

## Suggested Next Steps

1) Implement OVK recovery + recipient/amount checks with `require!`/`unwrap`.
2) Fix `to_zcash_tx()` so `get_hash_to_sign()` includes Orchard bundle in ZIP-244 digest.
3) Optionally add RedPallas signature verification after `set_input_utxo`.
4) Keep Halo2 verification behind a cargo feature and benchmark it.

## Appendix: Pseudocode Snippets

Policy checks during bundle parse (simplified):

```rust
let mut rdr = Cursor::new(orchard_bundle_bytes);
let bundle = read_v5_bundle(&mut rdr).unwrap().unwrap();
require!(bundle.actions().len() == 1, "Only one orchard action is supported");

let (note, addr, _memo) = bundle
    .recover_output_with_ovk(0, &orchard::keys::OutgoingViewingKey::from(BRIDGE_OVK))
    .unwrap();
require!(note.value().into_u64() == expected_amount, "Orchard amount mismatch");
require!(addr.to_string() == expected_recipient, "Orchard recipient mismatch");

#[cfg(feature = "orchard_proof_verify")]
{
    let vk = orchard::circuit::VerifyingKey::build();
    bundle.verify_proof(&vk).unwrap();
}
```

Including Orchard in ZIP-244 txid parts (conceptual):

```rust
// Convert Authorized → EffectsOnly for TransactionData
let effects_bundle = bundle.map_authorization(
    &mut (),
    |_ctx, _auth, _sa| (),
    |_ctx, _auth| orchard::bundle::EffectsOnly,
);

let tx_data = TransactionData::from_parts(
    TxVersion::V5,
    branch_id,
    0,
    BlockHeight::from_u32(expiry_height),
    Some(transparent_bundle),
    None,
    None,
    Some(effects_bundle),
);
```

Verifying RedPallas (outline):

```rust
// After set_input_utxo (PCZT known) and tx_data created with Orchard included
let txid_parts = tx_data.digest(zcash_primitives::transaction::txid::TxIdDigester);
let sighash = zcash_primitives::transaction::sighash::signature_hash(
    &tx_data,
    &zcash_primitives::transaction::sighash::SignableInput::All, // conceptual placeholder
    &txid_parts,
);

for action in bundle.actions().iter() {
    let rk: orchard::primitives::redpallas::VerificationKey<orchard::primitives::redpallas::SpendAuth> = action.rk().clone();
    let sig = action.authorization(); // per-action SpendAuth signature
    redpallas::verify_prehashed(&rk, sighash.as_ref(), &sig).unwrap();
}

let cv_net = bundle.cv_net();
let binding_vk = derive_binding_vk(cv_net); // via orchard helper
let binding_sig = bundle.authorization().binding_signature();
redpallas::verify_prehashed(&binding_vk, sighash.as_ref(), &binding_sig).unwrap();
```

Note: Replace placeholders with the exact API calls exposed by the pinned crate versions during implementation.
