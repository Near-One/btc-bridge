# Repository Guidelines

## Overview

NEAR smart contracts for a Bitcoin-to-NEAR bridge. Deposits are proven by parsing Bitcoin transactions; nBTC (NEP-141) is minted on NEAR. Withdrawals consume tracked UTXOs, produce a BTC transaction, and pass relayer-assisted signing/verification. Optional Zcash support is feature-gated. Mocks exist for deterministic end-to-end tests.

## Project Structure & Module Organization

- Root: `Cargo.toml`, `Makefile`, `.github/workflows/update-contracts.yaml`.
- Contracts under `contracts/`:
  - `satoshi-bridge/` — core bridge logic; tests in `tests/`.
  - `nbtc/` — NEP-141 wrapped BTC token.
  - `mock-*` — test scaffolding contracts.
- Build artifacts in `res/` (gitignored); Rust output in `target/`.

## Build, Test, and Development Commands

- `make build` — format, clippy-fix, and build all WASMs.
- `make release` — reproducible builds for tagged releases.
- `make clean` — remove `target/` and `res/`.
- `cargo test -p satoshi-bridge` — run integration tests (`near-workspaces`).
- Prereqs: `rustup target add wasm32-unknown-unknown`; install `cargo-near` via the script in CI.

### Building with Zcash Support

The satoshi-bridge contract supports optional Zcash features. To build with Zcash support:

```bash
# Build optimized WASM (recommended for deployment)
cargo near build non-reproducible-wasm --features zcash --out-dir res --no-abi

# Build for testing (unoptimized, faster builds)
cargo build -p satoshi-bridge --features zcash --release --target wasm32-unknown-unknown

# Run Zcash tests
cargo test -p satoshi-bridge --features zcash -- --test-threads=1 --nocapture
```

Notes:

- Use `cargo near build` with `--no-abi` flag because ABI generation fails with bitcoin types (missing JsonSchema implementations)
- The `--no-abi` build produces optimized WASM (~1.3MB) that fits within NEAR's 1.57MB contract size limit
- Regular cargo build produces unoptimized WASM (~2.1MB) that exceeds the limit - don't use it

## Coding Style & Naming Conventions

- Rust 2021; run `make lint` before pushing.
- `rustfmt` + `clippy` (aim for zero warnings).
- Names: modules/functions `snake_case`, types `PascalCase`, consts `SCREAMING_SNAKE_CASE`.
- Keep modules cohesive (e.g., `src/api`, `src/rbf`, `src/btc_light_client`, `src/zcash_utils`).

## Testing Guidelines

- Use `#[tokio::test]` with `near-workspaces` under `contracts/satoshi-bridge/tests/`.
- Descriptive file names (e.g., `test_upgrade.rs`); keep tests deterministic.
- Useful: `cargo test -p satoshi-bridge -- --nocapture` for logs.

## Commit & Pull Request Guidelines

- Commits: short, imperative; Conventional style welcome (`feat:`, `fix:`).
- PRs: description + rationale, linked issues, passing `make build` and tests, updated tests for behavior changes.
- Do not commit `res/` or `target/`. Tagging `btc-bridge-vX.Y.Z` triggers CI release with WASMs.

## Agent-Specific Instructions

- Scope changes to `contracts/*/src/` unless explicitly asked to modify CI/release.
- Preserve workspace deps and toolchain; avoid version bumps unless required.
- Prefer feature flags for optional logic (`zcash`), and keep state changes explicit and audited.
- Be conservative with on-chain compute; avoid panics, use clear error returns.
- For new behavior, add integration tests in `contracts/satoshi-bridge/tests/`.
