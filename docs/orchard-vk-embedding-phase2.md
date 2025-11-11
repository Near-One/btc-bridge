Title: Orchard Verifying Key Embedding (Phase 2) — Plan and Tasks

Summary
Runtime building of the Orchard VerifyingKey (VK) costs ≈300 Tgas on NEAR and regularly exceeds the per‑contract gas burn cap. Proof verification itself is inexpensive (≈10–20 Tgas), but only if a VK is available. We must move to embedding VK/params and deserializing them at runtime. This requires an Orchard fork that depends on a halo2 fork with plonk::VerifyingKey serialization support. We will base this on PSE’s halo2 fork (https://github.com/privacy-scaling-explorations/halo2). If VK serialization is not available at the chosen commit, we will add it there and pin to that commit.

You already forked orchard at ~/orchard. This plan uses that fork and vendors a halo2 fork locally to add VK serialization. We’ll then generate VK/params offline, embed them into the orchard‑verifier contract via include_bytes!, and verify proofs using deserialized VK (no runtime build).

Background and Findings
- Gas measurements (via near-workspaces, with full outcomes and logs):
  - build_vk_only: ≈301.5 Tgas (success)
  - parse_and_build_only (bundle parse + VK build, no verify): ≈301.6 Tgas (failure: Exceeded the maximum amount of gas allowed to burn per contract)
  - verify_orchard_bundle: earlier ~12 Tgas readings were from a failed receipt (single‑action panic). Logs showed it never reached VK build; that number is not a valid build+verify cost.
- Conclusion: VK build on-chain is not viable. We must embed VK and params, and deserialize them.

Goal
- Use your Orchard fork (~/orchard) and a local vendor halo2 fork to add VK serialization.
- Implement orchard::circuit::VerifyingKey::{to_bytes, from_bytes}.
- Generate VK/params once offline and embed them into orchard‑verifier with include_bytes!.
- Update the orchard‑verifier contract to load VK from bytes and verify proofs.
- Ensure all Orchard verification (including policy checks) are called from the external orchard-verifier contract.
- Confirm deploys and gas behavior via tests.

Repo State (after previous work)
- Main bridge (satoshi-bridge) calls an external orchard-verifier to perform proof + OVK/policy checks. Inline verification is disabled if an external verifier is configured.
- Orchard gas tests and the UA/bundle generator have been moved under orchard‑verifier:
  - tests/gas_verify.rs
  - tests/gas_parse_build.rs
  - tests/gas_vk_build.rs
  - tests/setup/{mod.rs, orchard.rs}
- These tests use near_workspaces::compile_project to build and deploy orchard‑verifier and print full outcomes/logs.

High‑Level Tasks
1) Vendor and patch halo2 with VK serialization (PSE fork)
2) Update your Orchard fork to depend on that halo2 and expose VerifyingKey::{to_bytes, from_bytes}
3) Add a generator tool to produce VK/params bytes
4) Embed VK/params in orchard‑verifier and load them at runtime
5) Update tests; measure gas; verify success paths
6) Document size/gas and any operational caveats

Detailed Plan

1) Vendor and patch halo2 with VK serialization (PSE fork)
- Create a local clone of PSE’s halo2 repository (or your fork of it):
  - Upstream: https://github.com/privacy-scaling-explorations/halo2
  - Path suggestion: ~/halo2_proofs
- Implement serialization for plonk::VerifyingKey<vesta::Affine> and any dependent types it needs (commitments, permutations, circuits metadata). You will likely need:
  - A stable write/read for:
    - Domain/capacity (K)
    - Circuit selector polynomial commitments
    - Gate configuration metadata used by VK
    - Permutation argument data/commitments
    - Instance column commitments
  - Use a binary encoding (e.g., version tag + lengths + little-endian numbers + affine points serialized as compressed bytes for curve points). Keep endianness consistent and document format.
- Expose a public API:
  - in halo2_proofs::plonk::VerifyingKey:
    - fn write<W: Write>(&self, writer: W) -> io::Result<()>
    - fn read<R: Read>(reader: R, params: &Params<vesta::Affine>) -> io::Result<Self>
  - Make sure VK::read validates shape vs. params, and returns errors on mismatched K/structures.

2) Update your Orchard fork at ~/orchard (point it to PSE’s halo2 fork)
- Edit ~/orchard/Cargo.toml to depend on your local clone of PSE’s halo2 fork:
  - [patch.crates-io]
    halo2_proofs = { path = "/home/ricky/halo2_proofs" }
    halo2_gadgets = { path = "/home/ricky/halo2_gadgets" }         # if needed
    halo2_poseidon = { path = "/home/ricky/halo2_poseidon" }       # if needed
  - Keep other dependencies unchanged unless the halo2 fork requires bumps.
- In orchard/src/circuit.rs, implement:
  - impl VerifyingKey {
      pub fn to_bytes(&self) -> Vec<u8> {
          // Use self.params.write and self.vk.write to serialize both
      }
      pub fn from_bytes(params_bytes: &[u8], vk_bytes: &[u8]) -> io::Result<Self> {
          // Params::<vesta::Affine>::read for params
          // VerifyingKey::<vesta::Affine>::read for vk
          // Construct orchard::circuit::VerifyingKey { params, vk }
      }
    }
- Ensure this compiles with the patched halo2. Adjust imports for Params read/write.

3) Generate VK/params bytes (offline)
- Add a small generator tool (either under orchard-verifier/tools or orchard/examples):
  - Steps:
    - use orchard::circuit::VerifyingKey::build() to get VK
    - serialize params and vk via to_bytes (alternatively separate param_bytes and vk_bytes)
    - write to files:
      - res/orchard_params.bin
      - res/orchard_vk.bin
    - Optionally compress: zstd or brotli to reduce WASM size
  - Output: (compressed) bytes + a small Rust module generator that emits a file orchard_vk_bytes.rs with:
    - pub const ORCHARD_PARAMS: &[u8] = include_bytes!("../res/orchard_params.bin.zst");
    - pub const ORCHARD_VK: &[u8] = include_bytes!("../res/orchard_vk.bin.zst");

4) Update orchard‑verifier to embed and load VK/params
- Add the generated module under contracts/orchard-verifier/src/zk_data/orchard_vk_bytes.rs (generated by your tool).
- Add a loader function:
  - If compressed:
    - Decompress params bytes and vk bytes (choose a no_std‑friendly decompressor; if needed, enable std in verifier).
  - Call orchard::circuit::VerifyingKey::from_bytes(&params_bytes, &vk_bytes) to reconstruct the VK.
- Change contract methods to use the embedded VK:
  - Remove or guard the call to VerifyingKey::build(), and instead call a get_cached_vk() that lazily deserializes once per transaction (or per method) and returns a reference to the reconstructed VK.
  - Verify proofs using the deserialized VK.
- Important: Do not rebuild or mutate VK/params at runtime; always deserialize from the constant blobs.

5) Update tests; measure gas and success
- tests/gas_vk_build.rs:
  - Replace build_vk_only with a method that deserializes VK only (vk_from_bytes_only) and measures gas. Expect a small number (<< 300 Tgas).
- tests/gas_parse_build.rs:
  - Replace parse_and_build_only with parse_and_load_only (parse bundle, load VK, derive instances, no verify) and measure gas. Expect small numbers.
- tests/gas_verify.rs:
  - verify_orchard_bundle should now:
    - parse bundle
    - load VK (from bytes)
    - verify proof
    - Log and assert outcome.is_success()
    - Expect total gas ≈ 10–20 Tgas for single‑action bundle
- Ensure tests print outcome, success/failures, and contract logs.

6) Size/headroom and final checks
- WASM size:
  - Record size before and after embedding VK/params. If too large, enable compression; confirm decompress CPU < few Tgas.
- Limits:
  - Ensure deserialization fits under per‑contract gas burn comfortably.
- Backward compatibility:
  - Pin orchard & halo2 commits in Cargo.lock; add [patch.crates-io] in orchard-verifier’s Cargo.toml pointing to your local forks for reproducibility.
- CI note:
  - Use compile_project in tests so this path works without prebuilt artifacts.

Commands and Workflow
- Build and patch forks:
  - cd ~/halo2_proofs; implement VK read/write; cargo build
  - cd ~/orchard; add [patch.crates-io] to point to ~/halo2_proofs; implement VerifyingKey::{to_bytes, from_bytes}; cargo build
- Generate VK/params:
  - cd contracts/orchard-verifier/tools
  - cargo run --bin gen_vk
  - This writes res/orchard_params.bin[.zst], res/orchard_vk.bin[.zst], and generates src/zk_data/orchard_vk_bytes.rs
- Build and test:
  - cargo test -p orchard-verifier -- --nocapture

Gas Targets
- VK deserialize only: << 50 Tgas (aim single-digit Tgas)
- Verify (single action): ≈ 10–20 Tgas
- Total per verify call: ≈ ~12–25 Tgas

Caveats and Notes
- Serialization format is internal. Document it carefully in the halo2 fork; ensure no accidental change across versions.
- params read/write is already available; match the params K exactly between generation and runtime.
- If orchard proofs change (e.g., circuit updates), regenerate VK/params and redeploy the verifier.
- WASM size: embedding uncompressed blobs may increase size significantly (MBs). Use compression if needed; test decompression gas cost.

What’s Already Done (in this repo)
- orchard‑verifier tests and setup moved and instrumented.
- External bridge offload path is in place; inline verify is suppressed when orchard_verifier_account_id is set.

Hand‑Off Checklist for the Agent
- Confirm local forks compile:
  - ~/halo2_proofs builds
  - ~/orchard builds with patches
- Implement VK read/write in halo2, and VerifyingKey::{to_bytes, from_bytes} in orchard::circuit.rs
- Add generator, embed blobs, wire orchard‑verifier to from_bytes
- Update and run gas tests; print outcomes/logs
- Report final gas and size numbers

End State Acceptance Criteria
- orchard-verifier uses deserialized VK, not runtime build.
- All three tests succeed:
  - vk_deserialize_only: success=true, small total_gas_burnt
  - parse_and_load_only: success=true, small total_gas_burnt
  - verify_orchard_bundle: success=true, total_gas_burnt ~10–20 Tgas for 1 action
- Bridge + verifier e2e can proceed without VK build and within gas limits.

