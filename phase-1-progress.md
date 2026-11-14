# Phase 1 Progress

Date: 2026-11-14
Branch: `phase-1/security-hardening`
Base commit: `a2dec0a`
Status: contract and agent hardening implemented; not deployed.

## Objective

Fix the four High security findings from the audit, harden the proof/oracle/liquidation/recovery paths, add adversarial tests, and document readiness. This phase does not add supplier deposits, liquidity pools, new chains/assets, frontend features, or live deployments.

## Completed

- Added chain-id validation: the contract now requires `chain_id == 11155111` at initialization.
- Rejected duplicate oracle keys at initialization and skipped duplicate configured keys during price updates.
- Rejected future oracle timestamps and treated stale/future prices as unhealthy.
- Redesigned liquidation accounting:
  - debt is not reduced when a liquidation request is accepted;
  - repayment is capped by collateral value and liquidation penalty;
  - full liquidation leaves a gas reserve instead of trying to transfer the entire balance;
  - debt reduction and bad-debt recognition happen only after confirmed Sepolia payout;
  - failed liquidation payouts refund the liquidator.
- Added liquidation refund states and a permissionless retry path:
  - `RefundPending`, `Refunding`, `Refunded`;
  - `refund_liquidation(position_id)` can retry a failed refund.
- Prevented user signatures from overwriting terminal or liquidation tickets.
- Preserved pending state when light-client hash verification mismatches; the position moves to `NeedsReview`.
- Restricted `clear_stuck_proof` to owner/guardian, added timeout requirements for unsigned operations, and rejected clearing signed payouts.
- Added callback correlation so stale `on_sign` and `on_block_hash` callbacks are ignored.
- Aligned relayer provider quorum:
  - minimum quorum is 2;
  - every provider is checked with `eth_chainId`;
  - providers not on Sepolia `0xaa36a7` are excluded;
  - proof collection fails closed below quorum.
- Aligned liquidator/keeper estimates with contract max-repay, gas-reserve, and bad-debt rules.
- Added/updated contract, relayer, liquidator, and keeper tests.
- Updated relayer/VERIFY/HANDOFF documentation to remove the single-provider workaround.

## Verification

- Contract unit tests: `43 passed`.
- Contract local E2E tests: `6 passed`.
- Relayer tests: `26 passed`.
- Keeper tests: `8 passed`.
- Liquidator tests: `12 passed`.
- Oracle tests: `4 passed`.
- Watcher tests: `16 passed`.
- Total recorded tests: `115 passed`.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p native-credit --all-targets -- -D warnings`: clean.
- `cargo check -p native-credit --target wasm32-unknown-unknown`: passed.

See `phase-1-test-report.md` for commands and scope limits.

## Not Done

- No contract deployment, upgrade, or live-state change was made.
- The existing v2 `Signed` liquidation ticket still has no legacy reconciliation path; new code treats it as active and `clear_stuck_proof` cannot clear it.
- Two-provider live Sepolia proof generation was not re-run; public-provider availability remains an operational dependency.
- The live v2 ticket remains `Signed`; the live v1 position remains `NeedsReview`.
- No fuzzing, formal verification, fallback oracle, timelock, or full event schema was added.
- Production wasm build and deployment review are still required.

## Next

1. Commit and push this milestone.
2. Add a one-time, private legacy-ticket migration for the existing v2 `Signed` ticket and test it.
3. Recover the v1 testnet balance, then retire v1 and move v2's guardian off the v1 account.
4. Build and review the deployment artifact; snapshot/export live state before any upgrade.
5. Re-run the live proof path with two independent Sepolia providers.
6. Reconcile the live v2 withdrawal with a finality proof; do not clear it as an unsigned stuck proof.
7. Decide whether to address remaining Medium/Low findings in Phase 2.
