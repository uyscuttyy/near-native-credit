# Audit Finding Status

Baseline: `AUDIT/AUDIT.md` (audit-baseline `f8241dc`), corrected by
`AUDIT/FOLLOWUP.md`. This file tracks the current status of each finding as of
2026-11-15 on branch `phase-1/security-hardening`.

## High findings

| Finding | Status | Evidence |
| --- | --- | --- |
| On-chain trie proof gas/size unmeasured | Measured on-chain from live transactions | Payload sizes are measured and bounded by `crypto::tests::measures_captured_proof_and_tx_payload_sizes`: header 688 B, total account proof 10,735 B, largest node 532 B, raw tx 121 B. On-chain gas was measured from the two live `confirm_withdrawal` transactions: the contract call that parses the header, verifies the account proof (trie verification), and settles state burned ~2.37 Tgas (v2, tx `CWHvU2SN8RntCU4srqdY4GBjCxLCf8W7fpRro7xjR4fc`) and ~2.47 Tgas (v1, tx `CKU2wZWaDWhaHdwxNxXFm7sUhfPbEUzwCRyoKRXvtHQC`); the light-client consensus receipt added ~8.5 Tgas; the total user cost per confirmation is ~15.3 Tgas (~0.0015 NEAR), far below NEAR's per-receipt gas cap. |
| Only one reliable keyless Sepolia RPC provider | Mitigated | `agents/relayer` enforces a minimum quorum of 2, validates `eth_chainId` per provider, and fails closed below quorum. Live two-provider proofs succeeded with thirdweb + onfinality at finalized Sepolia block 12133303 (quorum 2/2). Provider health remains an operational dependency. |

## Medium findings

| Finding | Status | Evidence |
| --- | --- | --- |
| `chain_id` configured as `1115711` | Fixed / not reproducible | `AUDIT/FOLLOWUP.md` §3 could not reproduce the typo; live config returns `11155111`. The contract now rejects any other chain id at init (`init_rejects_sepolia_chain_id_typo`), and the mutation harness kills the removed-validation mutant. |
| `min_withdrawal_wei` margin thin vs gas spikes | Fixed | Init and `set_gas_limit` both require `min_withdrawal_wei > max_gas_wei`. With the deployed config (gas limit 250,000, max fee 2 gwei), `max_gas_wei` is 5e14 = exactly 1/20 of the 1e16 minimum, enforced by `min_withdrawal_keeps_a_20x_margin_over_max_gas`; `gas_limit_cannot_exceed_min_withdrawal_headroom` rejects gas-limit increases that would consume the headroom. |
| No mutation tests | Fixed | `scripts/mutation_tests.py` mutates 9 security guards (chain id, duplicate oracle keys, future price, stale health, storage deposit, terminal ticket, `set_gas_limit` owner guard, `set_gas_limit` headroom, signed-payout reconciliation) and requires the matching test to fail. All 9 mutants are killed. |
| Watcher/keeper/liquidator lack tests | Fixed | Watcher 19, keeper 8, liquidator 12 tests pass, covering risk evaluation, position classification, liquidation planning, receipt alerts, signed payouts, and refund states. |

## Low findings

| Finding | Status | Evidence |
| --- | --- | --- |
| `liquidation_ticket` uses an internal claim, not NEP-171 | Closed by design | Internal claims avoid a separate NFT contract and keep the credit-line engine single-contract. Documented in `HANDOFF.md`; no change planned for this phase. |
| Frontend is Vite+React, not Next.js | Closed by design | The dashboard is a lightweight viewer; migrating frameworks adds no security value. Documented in `HANDOFF.md`. |
| `clear_stuck_proof` timeout uses `block_height`, not timestamp | Closed by design | NEAR block height is monotonic and strictly time-bounded, so it cannot be manipulated like node-local timestamps. The ~100,000-block timeout (~31-35 h at ~1 s/block) is enforced identically for pending deposits and unsigned withdrawals; signed payouts are always rejected regardless of timeout. |

## Phase 1 items still open (tracked in phase docs)

- On-chain gas measurement for `verify_trie_proof` (needs a live call).
- Operational alert delivery for the watcher (webhook exists; no ops channel wired).
- Oracle fallback feed, deviation/velocity circuit breaker, key rotation, timelock, NEP-297 event schema, indexer, and differential verifier testing are Phase 2 work items per `recommended-roadmap.md`.
