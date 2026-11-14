# Native Credit: Recommended Roadmap

**Audit snapshot:** 2026-11-14  
**Baseline commit:** `f8241dc`  
**Purpose:** Sequence the work from the current testnet credit-line prototype to a defensible market product. This is a recommendation, not an authorization to change code or live state.

---

## 1. Planning assumptions

1. The current architecture is a **credit-line engine**, not a pooled money market.
2. The proof and MPC design is worth preserving; the failure paths and product layer are the main gaps.
3. No production or third-party-user deployment should happen before the High findings are fixed and independently reviewed.
4. The live v2 position is in `WithdrawalPending` with a successful Sepolia payout and a `Signed` liquidation ticket. It should be reconciled only after audit approval.
5. The v1 position is in `NeedsReview` and must be either reconciled or explicitly retired.
6. Adding supplier accounting to the current per-position state layout is a migration-level change, not a small feature.

---

## 2. Decision gates

| Gate | Decision | Required evidence |
|---|---|---|
| G0 | Accept or reject the audit findings and pause further live use | Written approval; owner named for each finding; no ambiguous live state |
| G1 | Approve the P0 technical design | Failure-path state machine, refund/claim design, ticket terminality, recovery design |
| G2 | Approve P0 implementation for testnet deployment | Regression tests, invariant tests, live-state reconciliation plan, rollback plan |
| G3 | Approve an audit-ready prototype | Independent review, CI, monitoring, runbooks, no open High findings |
| G4 | Approve a supplier-market design | Economic model, loss waterfall, caps, oracle/risk design, migration plan |
| G5 | Approve production | External audits, operational readiness, incident drills, legal/compliance review |

No gate should be skipped because "it is only testnet." The current defects are about fund handling and accounting semantics, not just UI.

---

## 3. Phase 0: Stop the bleeding

**Goal:** Prevent further loss or state divergence while the design is fixed.  
**Duration:** Immediate, before any new feature work.  
**Owner:** Contract lead + operations lead.

### P0.1 Freeze risk-increasing activity

- Do not accept third-party supplier funds. There is no supplier product yet.
- Do not onboard external borrowers beyond controlled test accounts.
- Do not run further live withdrawals/liquidations until the P0 design is approved.
- Keep read-only monitoring active.
- Record the current live state as the audit baseline.

### P0.2 Fix liquidation fund handling (F-01)

- Decide the atomicity model: either
  - reduce debt only after a proven Sepolia payout, or
  - reduce debt immediately but create a first-class liquidator claim.
- Add a refund/retry path that cannot be blocked by a single owner.
- Preserve the liquidator's principal and status across signing, broadcast, success, failure, and timeout.
- Add tests for signing failure, broadcast failure, revert, replacement, and reorg.
- Emit events for claim creation, signing, broadcast, success, failure, and refund.

**Exit criteria:** A liquidator cannot lose USDC without a recorded, actionable claim or refund.

### P0.3 Make liquidation tickets terminal (F-02)

- Set `Signed` only when the pending withdrawal is a liquidation and the ticket is `PendingSignature`.
- Make `Settled` and `Cancelled` terminal.
- Clear the active ticket after terminal settlement, or permit transfer when the ticket is terminal and debt is zero.
- Add a ticket-history or event log.
- Add the regression test: settle a liquidation, sign a user withdrawal, assert the ticket stays `Settled`.

**Exit criteria:** No later signature or user action can overwrite a terminal ticket.

### P0.4 Record bad debt (F-03)

- Compute the shortfall when seized collateral is worth less than the debt reduction.
- Store per-liquidation `bad_debt_usdc` and increment `total_bad_debt_usdc`.
- Emit a bad-debt event.
- Define the loss-allocation policy even if the initial implementation is "protocol reserve/test treasury."
- Add invariant tests: debt reduction equals cash received plus recognized bad debt.

**Exit criteria:** A liquidation with insufficient collateral produces an explicit, auditable deficit.

### P0.5 Build safe recovery for signed/unbroadcast operations (F-04)

- Model payout states: `Unsigned`, `Signed`, `Broadcast`, `ProvenSuccess`, `ProvenFailure`, `Expired`.
- Do not clear a signed payout without a chain receipt or proof.
- On any light-client mismatch, set `NeedsReview` and preserve all pending fields.
- Add a bounded timeout that moves the position to a safe review state without losing the claim.
- Add nonce reconciliation: check whether the signed transaction hash was mined before allowing a replacement.
- Restrict `clear_stuck_proof` so it cannot desynchronize collateral, nonce, debt, or tickets.
- Add tests for every failure path above.

**Exit criteria:** There is no state in which a signed payout is invisible, unretryable, or safely clearable without chain reconciliation.

### P0.6 Require real proof quorum (F-06)

- Collect proofs from at least two independent providers that agree on block hash and account state.
- Pin and compare block hashes, not just block numbers.
- Add a local/archive node or a reliable paid provider for finalized `eth_getProof`.
- Fail closed and alert when quorum is unavailable.
- Remove `--min-quorum 1` from any user-facing or default path.

**Exit criteria:** A single RPC provider cannot unilaterally determine proof finalization.

### P0.7 Reconcile live v2 and v1 state after approval

**v2**

- Use the Sepolia receipt for tx `0x2a90b77ad94583af484e0f25fdf3f33a69c4d9e60869fbf7830b86738271f58f` and a fresh account proof to confirm the pending withdrawal.
- Expected classification: success, because the observed balance delta is about `0.0102046 ETH`, between `0.01` and `0.0105`.
- After confirmation, verify position status, collateral, nonce, and ticket state.
- Repair or migrate the `Signed` ticket state only after the P0 ticket fix is reviewed.

**v1**

- Either reconcile the `NeedsReview` position with a proof and a documented decision, or explicitly retire it and mark it as abandoned.
- Do not leave it in an unexplained state.

**Exit criteria:** Every live position has a documented state, a transaction trail, and an owner-approved resolution.

### P0.8 Tell the truth in docs

- Update `README.md`, `HANDOFF.md`, and `VERIFY.md` to match live state:
  - current v2 status is `WithdrawalPending`, not `Idle`;
  - current v2 collateral is `0.013897999998392 ETH`, not `0.003897999998392 ETH`;
  - the v2 ticket is `Signed`, not `Settled`;
  - the Sepolia payout succeeded but the NEAR confirmation is pending;
  - v1 is `NeedsReview`.
- Remove the stale source comment claiming the deployed v2 gas limit is 60,000.
- Do not claim an audit baseline tag that does not exist.

**Exit criteria:** A new operator can read the docs and reproduce the live state without contradictory claims.

---

## 4. Phase 1: Audit-ready prototype

**Goal:** Make the existing credit-line engine defensible and observable.  
**Duration:** After P0, before supplier-market work.  
**Owner:** Contract lead + security lead.

### P1.1 Harden the oracle (F-05)

- Reject duplicate oracle keys and duplicate signatures.
- Require distinct signers and record which keys signed.
- Reject future-dated prices or tightly bound them to block time.
- Make `get_health` report stale prices instead of silently using them.
- Add a fallback feed and a deviation/velocity circuit breaker.
- Document key custody, rotation, and incident response.
- Emit price events with sequence, timestamp, and signer set.

**Exit criteria:** A single key cannot satisfy the threshold; stale prices are visible; a feed failure has a defined response.

### P1.2 Harden the verifier (F-07)

- Add fuzzing for RLP, MPT nodes, account decoding, and EIP-1559 encoding.
- Differential-test against a mature reference implementation.
- Add property tests for proof soundness and completeness.
- Document accepted/rejected edge cases.
- Consider an audited verifier or a verified account-state API.
- Keep account-proof scope explicit; do not imply storage-proof support.

**Exit criteria:** The verifier has more than happy-path vectors; fuzzing and differential tests run in CI.

### P1.3 Add events and an indexer

- Emit NEP-297 or documented NEAR events for:
  - position creation, address registration, deposit proof, deposit credit;
  - borrow, repay, interest accrual;
  - withdrawal request, signature, broadcast, confirmation, failure;
  - liquidation request, ticket transitions, payout, bad debt;
  - oracle updates;
  - admin changes.
- Add an indexer or subgraph for position history and transaction evidence.
- Make the frontend read from the indexer rather than hardcoded links.

**Exit criteria:** A user or auditor can reconstruct a position's history from events.

### P1.4 Add governance and operational controls (F-08)

- Multisig owner and upgrade authority.
- Two-step ownership/guardian transfer.
- Timelock for parameter changes.
- Bounded parameter changes.
- Explicit emergency modes: pause new risk, pause all, recovery.
- Document who can do what and how to escalate.
- Remove stale config comments and generate docs from live state.

**Exit criteria:** No single key can silently change critical parameters or upgrade code.

### P1.5 Add observability

- Metrics for pending operations, proof lag, provider success, oracle freshness, health, and liquidatable positions.
- Alerts for `NeedsReview`, stale price, proof quorum failure, failed broadcast, and policy mismatch.
- Dashboards for the live deployment.
- Structured logs with correlation ids.
- Runbooks for each alert.

**Exit criteria:** The team can detect and diagnose the failure modes in this review without manual chain archaeology.

### P1.6 Build the test and CI bar

- CI for contract, agents, and web.
- Failure-path tests for F-01 through F-04.
- Invariant tests for totals and bad debt.
- Storage migration tests.
- Live-orchestration tests with all agents.
- Coverage reporting.
- Independent security review.

**Exit criteria:** Green CI is a meaningful gate, not just a compilation check.

---

## 5. Phase 2: Usable testnet market

**Goal:** Move from a single-position demo to a market that third-party testnet users can use.  
**Dependency:** P1 complete and independently reviewed.  
**Owner:** Product lead + contract lead + risk lead.

### 5.1 Decide the market architecture

Before writing supplier code, decide:

- **Option A:** Keep the credit-line engine and add a separate supplier/market contract that funds it.
- **Option B:** Build a new pooled market contract and treat the current contract as a migration source or legacy product.
- **Option C:** Upgrade the current contract with a new storage layout and migration.

Recommendation: **Option B or a deliberate separation**. Bolting LP shares onto per-position debt and a single ticket field will create accounting and migration risk. Whatever is chosen, document the migration and the loss waterfall first.

### 5.2 Supplier market

- Supplier deposit and withdrawal.
- LP shares or receipt tokens.
- Utilization-based interest-rate model with a kink.
- Reserve factor and protocol fee.
- Supplier yield accrual and claim.
- Supply caps and per-market caps.
- Bad-debt socialization policy.
- Withdrawal liquidity constraints and queued withdrawals if needed.
- Supplier risk disclosures.

**Exit criteria:** A supplier can deposit, earn yield, and withdraw under documented liquidity conditions; losses are allocated by a specified rule.

### 5.3 Risk and liquidation engine

- Close factor.
- Liquidation auction or Dutch mechanism, or a documented fixed-penalty rationale.
- Stability pool or insurance fund.
- Bad-debt waterfall.
- Per-asset risk parameters.
- Oracle fallback and circuit breakers.
- Liquidation UI and keeper integration.
- Grace period/cooldown policy.
- Recovery for failed liquidation payouts.

**Exit criteria:** A liquidation cannot silently socialize an unrecognized loss; liquidators have a predictable claim.

### 5.4 Product UX

- Create and discover positions.
- Deposit address, QR, and proof status.
- Borrow/repay with health preview.
- Withdraw request, signature status, broadcast, and confirmation.
- Payout-address registration.
- Liquidation interface.
- Test USDC onboarding.
- Transaction history from events.
- Clear testnet labeling and risk warnings.
- Accurate live state generated from the chain.

**Exit criteria:** A new testnet user can complete the lifecycle without an operator manually constructing calls.

### 5.5 Multi-position and scale

- Per-owner position index.
- Pagination for discovery.
- Storage lifecycle and refunds.
- Bounded scans.
- Position limits or rate limits if needed.

**Exit criteria:** The UI and agents do not depend on scanning all positions.

---

## 6. Phase 3: Production and universal-market path

**Goal:** Move toward the long-term cross-chain money market vision.  
**Dependency:** P2 complete, external audits, and a production operating model.

### 6.1 Multi-asset and multi-chain

- Multiple collateral assets.
- Multiple debt assets.
- Chain-specific light clients and proof adapters.
- Contract-based collateral support, which requires storage proofs or a verified state API.
- Cross-chain liquidation routing.
- Chain abstraction for users.
- Per-chain risk limits and governance.

### 6.2 Decentralization and governance

- Decentralized or independent oracle set with fallback.
- Timelocked parameter governance.
- Transparent upgrade process.
- Key rotation and signer accountability.
- Emergency response with defined authority.
- Public incident disclosures.

### 6.3 Security assurance

- Multiple independent audits.
- Formal verification of critical invariants.
- Continuous fuzzing.
- Bug bounty.
- Operational security review of MPC, RPC, and key custody.
- Incident-response drills.
- Mainnet canary and staged caps.

### 6.4 Economics and compliance

- Interest-rate and reserve calibration.
- Bad-debt stress testing.
- Liquidity and redemption stress testing.
- Legal/regulatory review for the target jurisdictions.
- Terms, disclosures, and sanctions screening as required.
- Treasury and reserve management.

**Exit criteria:** A production readiness review signs off on security, economics, operations, and compliance. Passing unit tests is not sufficient.

---

## 7. Workstream sequencing

```text
G0  Accept findings, freeze risk
 │
 ├─ P0.1 freeze
 ├─ P0.2 liquidation refund/claim
 ├─ P0.3 ticket terminality
 ├─ P0.4 bad debt
 ├─ P0.5 recovery state machine
 ├─ P0.6 proof quorum
 ├─ P0.7 live reconciliation
 └─ P0.8 docs truth
        │
        ▼
G1  Approve P0 design
        │
        ▼
P1  Oracle, verifier, events, governance, observability, CI
        │
        ▼
G2  Approve testnet deployment of P0/P1
        │
        ▼
G3  Independent review
        │
        ▼
P2  Supplier market, risk engine, UX, scale
        │
        ▼
G4  Approve market design
        │
        ▼
P3  Multi-asset, multi-chain, decentralization, production
        │
        ▼
G5  Production approval
```

P0 work can be split across contract and operations, but P0.2 through P0.5 should be designed together because they share the payout/claim state machine.

---

## 8. Finding-to-work mapping

| Finding | P0/P1 task | Exit evidence |
|---|---|---|
| F-01 failed liquidation loss | P0.2 | Refund/claim test; no unclaimed USDC loss |
| F-02 ticket overwrite | P0.3 | Terminality regression test; live state repaired |
| F-03 bad debt | P0.4 | Deficit recorded; invariant test |
| F-04 stuck/recovery | P0.5 | Timeout/reconciliation tests; no unsafe clear |
| F-05 oracle | P1.1 | Unique keys; stale health; fallback |
| F-06 single provider | P0.6 | Two-provider quorum; fail-closed |
| F-07 verifier assurance | P1.2 | Fuzz/differential tests; independent review |
| F-08 admin centralization | P1.4 | Multisig, timelock, events, pause modes |
| F-09 ticket lifecycle | P0.3 + P1.3 | Terminal ticket; transfer test; event history |
| F-10 unbounded scan | P2.5 | Index/pagination; bounded query |
| F-11 storage lifecycle | P2.5 | Close/refund/cleanup |
| F-12 demo UI | P2.4 | Self-serve lifecycle; live state |
| F-13 no supplier market | P2.1-2.3 | Supplier product + risk model |

---

## 9. Do-not-do list

Do not:

- deploy to Ethereum mainnet or NEAR mainnet with the current High findings;
- accept third-party supplier funds into the current contract;
- use `--min-quorum 1` for user-facing proof flows;
- clear `NeedsReview` or pending state without checking the Sepolia chain;
- treat `total_bad_debt_usdc` as implemented because the field exists;
- trust `README.md`/`HANDOFF.md` claims of `Idle`/`Settled` without querying live state;
- add LP shares to the current per-position storage layout without a migration design;
- repair live state before the audit findings are approved;
- treat the existing test suite as an independent security audit.

---

## 10. Definition of done by phase

### P0 done

- A liquidator cannot lose USDC without a recorded claim/refund.
- Terminal tickets cannot be overwritten.
- Bad debt is recorded.
- Signed/unbroadcast payouts have a safe, documented recovery path.
- Proof quorum is at least two independent providers.
- Live v2 and v1 states are documented and reconciled or explicitly retired.
- Docs match live state.

### P1 done

- Oracle duplicate/staleness issues are addressed or explicitly risk-accepted.
- Verifier has fuzzing/differential tests and an independent review.
- Events/indexer make history reconstructable.
- Admin actions are timelocked, multisig-controlled, and observable.
- Monitoring and runbooks cover the known failure modes.
- CI enforces the regression and invariant tests.

### P2 done

- Suppliers can deposit, earn, and withdraw under documented rules.
- Interest, reserves, and bad debt have a coherent accounting model.
- Liquidations have a predictable claim and loss waterfall.
- Users can complete the lifecycle without operator intervention.
- The market has caps, oracle fallback, and risk limits.

### P3 done

- Multi-asset and multi-chain support is audited and live-tested.
- Governance and key management are production-grade.
- External audits and formal verification are complete.
- Incident response has been drilled.
- The economics and compliance model are approved for the target market.

The shortest safe path is not "add more features." It is: fix the fund-handling semantics, make failures observable and recoverable, then add the market layer on top of a trustworthy engine.
