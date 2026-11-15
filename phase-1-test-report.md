# Phase 1 Test Report

Date: 2026-11-14
Branch: `phase-1/security-hardening`
Result: all recorded local tests pass. This report covers local checks; live v1/v2 reconciliation and cleanup deployment are recorded in `phase-1-operational-readiness.md`. Audit finding statuses are tracked in `AUDIT/STATUS.md`.

## Summary

| Suite | Tests | Result |
|---|---:|---|
| Contract unit (`--lib`) | 52 | Pass |
| Contract local E2E | 6 | Pass |
| Relayer | 26 | Pass |
| Keeper | 8 | Pass |
| Liquidator | 12 | Pass |
| Oracle | 4 | Pass |
| Watcher | 19 | Pass |
| **Total** | **127** | **Pass** |

Static checks:

- `cargo fmt --all -- --check`: pass.
- `cargo clippy -p native-credit --all-targets -- -D warnings`: pass.
- `cargo build -p native-credit --target wasm32-unknown-unknown --release`: pass.

## Commands

Contract:

```sh
cd contracts/native-credit
cargo test -p native-credit --lib
cargo test -p native-credit --test local_e2e
cargo fmt --all -- --check
cargo clippy -p native-credit --all-targets -- -D warnings
cargo build -p native-credit --target wasm32-unknown-unknown --release
```

Agents:

```sh
cd agents/relayer && npm test
cd agents/keeper && npm test
cd agents/liquidator && npm test
cd agents/oracle && npm test
cd agents/watcher && npm test
```

## Contract coverage

### Unit tests

The 52 unit tests cover:

- initialization, storage deposit, and config validation;
- chain-id typo rejection (`1115711` vs `11155111`);
- duplicate oracle key rejection;
- price quorum validation, future-timestamp rejection, and stale-price health;
- deposit proof verification and light-client hash confirmation;
- withdrawal signing, signer mismatch, and terminal-ticket protection;
- transfer of idle and terminal-ticket positions;
- liquidation math, penalty capping, max repay, and token conversion;
- proof/tamper vectors, EIP-1559 hashing, EOA detection, and signature recovery;
- risk-off behavior and deployed gas-limit controls.
- Withdrawal-minimum headroom: the 0.01 ETH minimum is at least 20x `max_gas_wei`, and `set_gas_limit` cannot consume that headroom.
- A liquidation `on_sign` path that cannot overwrite a terminal ticket with a valid MPC-style signature.

### Local E2E tests

The 6 local E2E tests cover:

- deposit, borrow, repay, withdraw, and finality confirmation;
- liquidation signature failure, refund callback failure, permissionless retry, and successful refund;
- successful liquidation repaying debt and settling the ticket;
- failed liquidation payout refunding the liquidator principal;
- underwater liquidation recording bad debt;
- proof mismatch preserving state and requiring reconciliation for a signed payout.

Key expected values from the underwater test:

- debt: `4_800_000` USDC base units;
- accepted liquidation repayment: `4_629_629`;
- bad debt: `170_371`;
- collateral before: `50_000_000_000_000_000` wei;
- seized collateral: `49_500_000_000_000_000` wei;
- gas reserve remaining: `500_000_000_000_000` wei;
- final debt: `0`;
- final `total_bad_debt_usdc`: `170_371`.

## Agent coverage

### Relayer

- Two-provider matching quorum.
- Provider mismatch rejection.
- Insufficient quorum rejection.
- Single-provider configuration rejection.
- Wrong-chain provider rejection.
- Real account, contract-account, absence, and tampered proof vectors.
- Light-client Borsh call construction and hash verification.
- NEAR view decoding, transaction failover, and reconciliation helpers.
- `near-cli` argument construction.

### Liquidator and keeper

- Collateral-capped repayment.
- Gas-reserve-aware seized collateral.
- Expected bad-debt reporting.
- Partial repayment behavior and USDC balance checks.
- Stale-price and pending-operation blocking.
- Existing CLI command construction and table rendering.

### Oracle and watcher

- Ed25519 price message construction and signing order.
- Missing-key gaps and signature counting.
- Base58 seed handling.
- Raw transaction integrity and Sepolia receipt alerts.
- Signed liquidation payout, refund-in-flight, stuck-refund, and refunded-state alerts.

### Mutation tests

`scripts/mutation_tests.py` mutates 9 security guards and requires the matching
test to fail. All 9 mutants are killed: chain-id validation, duplicate oracle
keys, future price, stale health, storage deposit, terminal ticket,
`set_gas_limit` owner guard, `set_gas_limit` headroom, and signed-payout
reconciliation. The harness reuses a git worktree under `/tmp` and reverts each
mutation after the run.

### Property tests

`proptest` cases (64 runs each) cover `parse_header_fields` on arbitrary bytes,
truncated real Sepolia headers, `decode_account` on arbitrary bytes, malformed
trie-node arrays, and minimal RLP u128 round-trips. The custom verifier is
fuzzed for panic-freedom and deterministic rejection, but is still not
differentially tested against a reference implementation.

### Payload size measurement

`measures_captured_proof_and_tx_payload_sizes` measures the captured Sepolia
vectors: header 688 B, total account proof 10,735 B, largest proof node 532 B,
and raw EIP-1559 transaction 121 B. On-chain gas for `verify_trie_proof` still
requires a live call and remains open.

## Scope limits

- Contract E2E tests use synthetic Sepolia proofs; they do not broadcast transactions.
- A live two-provider Sepolia proof quorum was re-measured at block `12133303` after deployment.

- No fuzzing, property testing, differential testing, or formal verification was performed.
- The custom RLP/MPT/transaction verifier remains consensus-critical and under-assured.

## Tooling notes

- `cargo check -p native-credit` fails with near-sdk guidance; use the wasm target or tests.
- A wasm-target check can overwrite a debug test binary and cause `Exec format error`. Run `cargo clean -p native-credit` before re-running host tests if that happens.
- `cargo fmt` was applied after the final contract change; the recorded test results were obtained after formatting and the final behavior changes.
