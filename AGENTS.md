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
