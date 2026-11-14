# Native Credit

**Status: testnet prototype. The happy path works end to end, but the system is not production-ready and is not safe for third-party users yet.**

Native Credit is a testnet prototype for borrowing testnet USDC on NEAR against Sepolia ETH, without wrapping the ETH or handing custody to a bridge.

A NEAR position owns a deterministic Sepolia address derived through NEAR chain signatures. A NEAR contract verifies the position's Sepolia ETH balance using a finalized Ethereum light client and an EIP-1186 account proof. The position can then borrow testnet USDC. Repayment, liquidation, and collateral withdrawals are coordinated by the NEAR contract; ETH leaves the deposit address only through a transaction the contract constructed and a chain signature it requested.

This is a hackathon prototype, not a mainnet product.

---

## Audit status

A read-only internal audit was completed on **2026-11-14** against commit `f8241dc` and the live testnet state. The audit found:

- **0 Critical**
- **4 High**
- **5 Medium**
- **3 Low**
- **1 Informational**

The High findings are lifecycle and accounting failures, not a missing signature check:

1. A failed or stuck liquidation can consume the liquidator's USDC with no refund or recoverable claim.
2. Liquidation ticket status can be overwritten by later signatures; the live v2 ticket is currently `Signed` when it should be terminal.
3. Bad debt is never recorded; debt can be zeroed even when seized collateral is worth less than the repayment.
4. There is no safe recovery path for signed-but-unbroadcast payouts or light-client proof mismatches.

The live proof run also used a single RPC provider, not the intended multi-provider quorum.

**Verdict:** conditionally usable for a controlled testnet demo; not ready for third-party testnet users, production, or mainnet.

### Phase 1 remediation (local, not deployed)

Phase 1 fixes for the four High findings are implemented on `phase-1/security-hardening` and covered by local tests, but they are **not deployed** and the live state above is unchanged. The fixes add liquidation refunds and retry, terminal-ticket protection, bad-debt accounting, signed-payout reconciliation, a one-time private legacy-ticket migration, oracle validation, and a two-provider chain-checked proof quorum. See:

- [`phase-1-progress.md`](./phase-1-progress.md)
- [`phase-1-security-fixes.md`](./phase-1-security-fixes.md)
- [`phase-1-test-report.md`](./phase-1-test-report.md)
- [`phase-1-operational-readiness.md`](./phase-1-operational-readiness.md)

Read the full audit:

- [`audit-report.md`](./audit-report.md) - summary, evidence, scorecard, release readiness.
- [`current-architecture.md`](./current-architecture.md) - system design, data model, flows, live snapshot.
- [`security-review.md`](./security-review.md) - threat model, findings, invariants, test gaps.
- [`missing-features.md`](./missing-features.md) - what exists versus what is absent.
- [`recommended-roadmap.md`](./recommended-roadmap.md) - sequenced remediation and market roadmap.

---

## Current live state

### v2 (`v2.native-credit.testnet`)

| Item | Value |
|---|---|
| Wasm size | `609491` bytes |
| Wasm sha256 | `345e1c91175345e78fc5b49c4ea6b9b18f06ed9de7dd3fbb9447264b7f330efd` |
| Code hash | `4XRS7nEZJqD6JC5zbE35xMn2gG1D1PVi7yZrChHjxx3N` |
| Deploy tx | `AoaLM6MDSS2nsziqgmmgwhmjpAA65KdmMGCv7gYGBoJT` |
| Light client | `client-eth2.sepolia.testnet` |
| MPC | `v1.signer-prod.testnet` |
| Chain id | `11155111` |
| Gas limit | `250000` |
| Oracle | 3 Ed25519 keys, threshold `2` |
| USDC balance | `20 USDC` |

Position `1` is owned by `borrower.native-credit.testnet` and uses deposit address `0x81163cfa3c95b3691fdb4fb0766ecafcfa267999`.

Current position state:

| Item | Value |
|---|---|
| Status | `WithdrawalPending` |
| Collateral | `0.013897999998392 ETH` |
| Debt | `0 USDC` |
| Last verified block | `12080892` |
| Last verified nonce | `3` |
| Pending withdrawal | `0.01 ETH` to `0x0d5d9acdb6adf6dce00d14972f7ce7b1e99b8fa6` |
| Sepolia tx | `0x2a90b77ad94583af484e0f25fdf3f33a69c4d9e60869fbf7830b86738271f58f` |
| Sepolia receipt | block `12080980`, status `0x1`, `gasUsed=204600` |
| NEAR confirmation | **Not submitted** |
| Liquidation ticket | `Signed` (known defect; expected terminal state was `Settled`) |

The Sepolia payout succeeded, but the NEAR position remains `WithdrawalPending` until `confirm_withdrawal` is submitted. Live confirmation is intentionally paused pending audit approval.

### v1 (`native-credit.testnet`)

The v1 position was recovered on 2026-11-14. The stale pending withdrawal was cleared, the current finalized balance was credited, oracle sequence `11` refreshed the price, and `0.085904 ETH` was withdrawn to `0x0d5d...b8fa6` in Sepolia tx `0x353b6195c17b93f829cef096e6b5dbd77c3418e43f9da1dafad2796a8aa11a3b`. V1 is now `Idle` with debt `0` and only `0.000020999999664 ETH` gas dust remaining. V2 still uses the v1 account as guardian; that is the remaining v1 dependency.

### Gas note

Sepolia's EIP-8037 state gas makes a transfer to a fresh EOA cost about `207391` gas (`183600` state + `21000` execution). The original v2 `60000` limit failed. Existing-EOA transfers still cost `21000`. Live v2 is now configured with `250000`; the fresh payout retry consumed `204600`.

---

## What works

- Per-position Sepolia address derivation through the MPC.
- EIP-1186 account-proof verification against a light-client-confirmed header.
- EOA-only collateral checks.
- ETH deposit credit after `block_hash_safe` confirmation.
- Testnet USDC borrow and NEP-141 repay.
- Health factor, borrow limit, interest accrual, and liquidation math.
- EIP-1559 transaction assembly and MPC signature recovery.
- User withdrawal request, broadcast, and confirmation.
- Liquidation repay, collateral seizure, and Sepolia payout.
- Team-operated Ed25519 price quorum.
- Relayer, oracle, watcher, keeper, and liquidator agents.

## What is missing

- Supplier deposits, LP shares, lender withdrawals, and yield distribution.
- Utilization-based interest, reserves, and protocol fees.
- Bad-debt recognition, socialization, insurance, or stability pool.
- Close factor, liquidation auction, or other production liquidation design.
- Independent oracle fallback and key rotation.
- Multi-provider proof quorum in live operation.
- Storage proofs, multi-asset support, and multi-chain support.
- Events, indexing, and a self-serve lifecycle UI.
- Timelocks, multisig upgrade controls, and production incident response.
- Fuzzing, formal verification, and an independent external audit.

See [`missing-features.md`](./missing-features.md) for the full matrix.

---

## Architecture at a glance

```text
NEAR account
    │
    ├── create position ──▶ MPC derives Sepolia EOA
    │
    ├── send ETH on Sepolia ──▶ account proof + light-client header check
    │
    ├── borrow USDC on NEAR ◀── contract transfers testnet USDC
    │
    ├── repay USDC ──▶ ft_transfer_call("repay")
    │
    └── request withdrawal / liquidate
             │
             ├── MPC signs EIP-1559 transaction
             ├── relayer broadcasts on Sepolia
             └── fresh proof + light-client check finalizes state
```

The contract is position-based. Each position has one owner, one derived Sepolia address, one collateral balance, one USDC debt, and at most one pending cross-chain operation. There is no shared liquidity pool.

See [`current-architecture.md`](./current-architecture.md) for the full design.

---

## Repository layout

```text
contracts/native-credit/   NEAR lending contract and proof verifier
agents/relayer/            proof submission, broadcast, reconcile
agents/oracle/             2-of-3 Ed25519 price signer
agents/watcher/            read-only health, oracle, receipt alerts
agents/keeper/             health-factor monitor and liquidation candidates
agents/liquidator/         dry-run planner and USDC liquidation executor
web/                       Vite + React demo dashboard
```

## Local verification

```sh
cargo test -p native-credit
cargo clippy -p native-credit --all-targets -- -D warnings
cargo build -p native-credit --target wasm32-unknown-unknown --release

cd agents/relayer
npm install --no-audit --no-fund
npm test

cd ../oracle
npm test

cd ../watcher && npm install --no-audit --no-fund && npm test
cd ../keeper && npm test
cd ../liquidator && npm test
```

Test counts at audit time:

| Suite | Result |
|---|---|
| Contract unit | 36 passed |
| Contract local E2E | 2 passed |
| Relayer | 24 passed |
| Watcher | 16 passed |
| Keeper | 7 passed |
| Liquidator | 10 passed |
| Oracle | 4 passed |
| Web | No test suite |

Passing tests are not a security audit. The High findings remain open.

## Run the demo dashboard

```sh
cd web
npm install --no-audit --no-fund
npm run dev
```

The app defaults to `v2.native-credit.testnet` and hardcoded position `1`. It is a demo, not the final self-serve UI. Override the contract, USDC, or RPC with `VITE_CONTRACT_ID`, `VITE_USDC_ID`, and `VITE_NEAR_RPC`.

The UI cannot create positions, discover all user positions, verify deposits, broadcast/confirm withdrawals, liquidate, or obtain test USDC. It also contains stale v1 transaction evidence.

---

## Documentation

Current authoritative documents:

- [`audit-report.md`](./audit-report.md) - audit summary and release verdict.
- [`current-architecture.md`](./current-architecture.md) - architecture and live state.
- [`security-review.md`](./security-review.md) - findings and recommendations.
- [`missing-features.md`](./missing-features.md) - capability gaps.
- [`recommended-roadmap.md`](./recommended-roadmap.md) - remediation sequence.

Historical/background documents:

- [`RESEARCH.md`](./RESEARCH.md) - Phase 0 research and design decisions.
- [`HANDOFF.md`](./HANDOFF.md) - historical run log; its final-state claims are stale.
- [`VERIFY.md`](./VERIFY.md) - verification commands and captured vectors; some live-state claims are stale.
- [`AUDIT/AUDIT.md`](./AUDIT/AUDIT.md) - prior audit baseline; not the current audit.
- [`AUDIT/FOLLOWUP.md`](./AUDIT/FOLLOWUP.md) - prior live-evidence follow-up.

When documents disagree, trust the on-chain state and the audit report above.

---

## Trust labels

- `REAL TESTNET`: Sepolia, NEAR testnet, chain signatures, the light client, and Circle testnet USDC.
- `PARTLY SIMULATED`: the team-operated price oracle.
- `DEMO`: seeded positions, scripts, or mock data.

This is not a mainnet product and must not be presented as one. No supplier market exists yet; the current contract is a credit-line engine.

---

## Next steps

Before any further live user-facing action:

1. Approve the audit findings and assign owners.
2. Reconcile the v2 and v1 live positions.
3. Fix or explicitly guard the High findings.
4. Require at least two independent proof providers.
5. Add regression and invariant tests for the failure paths.
6. Keep this README, `HANDOFF.md`, and `VERIFY.md` synchronized with verified live state.

See [`recommended-roadmap.md`](./recommended-roadmap.md) for the P0/P1/P2/P3 sequence.

---

## Hackathon disclosure

Built for **NEAR HACKATHON** (2026-11-01 to 2026-11-14). The repository is new; the research and code were written in the hackathon window. Libraries and reference projects are listed in `RESEARCH.md`. The project is testnet-only.

## License

MIT. See [`LICENSE`](./LICENSE).
