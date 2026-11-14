# Native Credit: Missing Features

**Audit snapshot:** 2026-11-14  
**Baseline commit:** `f8241dc`  
**Purpose:** Separate what exists from what is genuinely absent. "Missing" here means not implemented, not merely under-tested or testnet-only.

---

## 1. Status legend

| Status | Meaning |
|---|---|
| **Implemented** | Present in code and exercised by tests or live evidence. |
| **Partial** | Present but limited, demo-only, or broken on important edge paths. |
| **Absent** | No implementation in the current repository. |
| **Out of scope** | Not expected for the current prototype phase. |

The current product is a **position-based, overcollateralized ETH/USDC credit line** on NEAR testnet and Ethereum Sepolia. It is not yet a pooled money market.

---

## 2. Summary matrix

| Domain | Capability | Status | Evidence / note |
|---|---|---|---|
| Capital | Supplier deposits | Absent | No supply or add-liquidity method |
| Capital | LP shares / receipts | Absent | No fungible LP token or share accounting |
| Capital | Withdraw supplied liquidity | Absent | No lender withdrawal path |
| Capital | Utilization-based interest | Absent | Only a fixed per-position annual borrow rate |
| Capital | Interest distribution to lenders | Absent | Borrow interest only increases debt |
| Capital | Protocol reserves / fees | Absent | No fee or reserve accounting |
| Capital | Bad-debt socialization | Absent | `total_bad_debt_usdc` is never updated |
| Capital | Debt ceiling / market caps | Absent | No per-market or global supply cap |
| Credit | Create position | Implemented | `create_position` with storage deposit |
| Credit | Derive Sepolia address | Implemented | MPC `derived_public_key` |
| Credit | Deposit ETH proof | Implemented | EIP-1186 account proof + light client |
| Credit | Borrow USDC | Implemented | Health and borrow-limit checks |
| Credit | Repay USDC | Implemented | `ft_transfer_call` `"repay"` |
| Credit | Withdraw ETH | Implemented | MPC-signed EIP-1559 transaction |
| Credit | Position transfer | Partial | Only idle, zero-debt, no-ticket positions |
| Credit | Close position | Partial | Automatic `Closed` state; no cleanup/refund |
| Credit | Multiple positions per user | Implemented | Position id per call |
| Credit | Position discovery in UI | Absent | UI hardcodes position `1` |
| Risk | Health factor | Implemented | Based on oracle price |
| Risk | Collateral factor | Implemented | Config bps |
| Risk | Liquidation threshold | Implemented | Config bps |
| Risk | Liquidation penalty | Implemented | Config bps |
| Risk | Partial liquidation | Implemented | Repay amount capped by debt |
| Risk | Close factor | Absent | Liquidator can repay full debt |
| Risk | Liquidation auction | Absent | Fixed penalty and first-come payout |
| Risk | Bad-debt recognition | Absent | Debt can be zeroed with a shortfall |
| Risk | Insurance / stability pool | Absent | No loss backstop |
| Risk | Risk caps per account | Absent | Only position-level limit |
| Risk | Isolation mode / e-mode | Absent | Single collateral/debt pair |
| Risk | Grace period / cooldown | Absent | No time-based liquidation protection |
| Oracle | Price update | Implemented | Ed25519 threshold message |
| Oracle | Team-operated quorum | Implemented | 2-of-3 in live deployment |
| Oracle | Independent fallback feed | Absent | No Chainlink/DEX TWAP fallback |
| Oracle | Duplicate-key rejection | Absent | Constructor does not enforce uniqueness |
| Oracle | Staleness flag in `get_health` | Absent | View uses stored price directly |
| Oracle | Key rotation / slashing | Absent | No on-chain rotation or penalty |
| Cross-chain | Sepolia account proof | Implemented | Custom MPT verifier |
| Cross-chain | Storage proof | Absent | Account proofs only |
| Cross-chain | Multi-provider proof quorum | Partial | Live run used single provider |
| Cross-chain | Automatic proof retry | Partial | Relayer reconciles, but no safe on-chain recovery |
| Cross-chain | Reorg handling | Partial | Relies on light-client safe block |
| Cross-chain | Multi-chain collateral | Absent | Sepolia only |
| Cross-chain | Multiple collateral assets | Absent | ETH only |
| Cross-chain | Multiple debt assets | Absent | USDC only |
| Agents | Relayer | Implemented | Tests and live use |
| Agents | Oracle signer/submitter | Implemented | Tests |
| Agents | Watcher | Implemented | Tests |
| Agents | Keeper | Implemented | Tests |
| Agents | Liquidator | Implemented | Tests and live liquidation |
| Agents | Metrics / alerting | Absent | No observability stack in repo |
| Agents | Scheduler / deployment manifests | Absent | No production orchestration |
| Agents | Idempotency guarantees | Partial | Reconcile logic exists; failure paths remain |
| Frontend | Wallet connect | Implemented | MyNearWallet testnet |
| Frontend | Borrow / repay / withdraw request | Implemented | Position `1` only |
| Frontend | Create position | Absent | No UI flow |
| Frontend | Discover positions | Absent | Hardcoded position |
| Frontend | Deposit proof flow | Absent | No UI or status |
| Frontend | Broadcast/confirm withdrawal | Absent | Relayer/CLI only |
| Frontend | Liquidation | Absent | Agent only |
| Frontend | Test USDC onboarding | Absent | No faucet/storage flow beyond storage registration |
| Frontend | Accurate history/events | Absent | No event log or indexer |
| Ops | Admin events | Absent | No event emission |
| Ops | Timelock / multisig | Absent | Owner/guardian are single accounts |
| Ops | Two-step ownership | Absent | No transfer function |
| Ops | Emergency shutdown | Partial | `risk_off` blocks new borrow/withdraw only |
| Ops | Upgrade/migration plan | Absent | NEAR account upgrade; no migration tooling |
| Ops | Key rotation | Absent | No documented or on-chain rotation |
| Data | NEP-297 events | Absent | No event log |
| Data | Indexer / subgraph | Absent | State queried directly |
| Data | Per-position history | Absent | Only current state and one ticket |
| Testing | Unit/E2E tests | Implemented | 38 contract tests; agent suites |
| Testing | Fuzzing / formal verification | Absent | No crypto fuzzing or proofs |
| Testing | Invariant tests | Partial | Some math tests; no accounting invariants |
| Testing | CI pipeline | Absent | No `.github` workflows found |
| Testing | Independent audit | Absent | This review is internal/read-only |
| Docs | Accurate live-state runbook | Partial | Docs contradict current live state |
| Docs | API/reference docs | Partial | READMEs and source comments |
| Docs | Incident response | Absent | No documented incident playbook |

---

## 3. Capital and market structure

### 3.1 Supplier side

**Status: Absent.**

There is no way for a lender to deposit USDC and earn yield. The contract does not:

- accept supplier liquidity;
- mint LP shares or receipts;
- track supplier principal;
- allow supplier withdrawals;
- calculate utilization;
- distribute borrower interest;
- charge a protocol reserve.

The live v2 contract holds 20 testnet USDC, but that is a pre-funded balance, not a supplier pool.

### 3.2 Interest and yield

**Status: Partial (borrower-side only).**

- Borrow interest is a simple annual rate in basis points (`interest_rate_bps`) accrued on the position debt (`math::accrue_interest`).
- It is not compounded per block and has no utilization curve.
- The interest increases borrower debt and `total_debt_usdc`.
- No lender receives that interest.
- There is no protocol fee, reserve factor, or treasury share.

### 3.3 Risk parameters and market caps

**Status: Absent.**

- No global debt ceiling.
- No per-position or per-user cap beyond the collateral factor.
- No supply cap.
- No asset-specific risk configuration because there is only one collateral and one debt asset.
- No isolation mode or efficiency mode.
- No borrow cap per asset.

### 3.4 Bad debt and reserves

**Status: Absent.**

- `total_bad_debt_usdc` exists but is never updated.
- A liquidation can zero debt even when seized collateral is worth less than the repayment.
- There is no reserve, insurance fund, stability pool, or socialization mechanism.
- There is no deficit event or per-position loss record.

---

## 4. Credit lifecycle

### 4.1 Position creation and onboarding

**Status: Implemented, with UI gaps.**

On-chain:

- `create_position` requires a fixed 0.02 NEAR storage deposit.
- It derives a Sepolia address through the MPC.
- It supports `retry_derive` if the callback fails.

Missing:

- UI flow for creating a position.
- QR code or copyable deposit address.
- Deposit instructions and safety warnings.
- Deposit detection/proof status in the UI.
- Position discovery by owner in the UI.
- Recovery guidance for derivation failure.

### 4.2 Deposits

**Status: Implemented on-chain; manual operationally.**

- The user must send Sepolia ETH externally.
- A relayer or operator must supply the header and account proof.
- The contract waits for the light-client hash check.
- There is no automatic deposit watcher integrated into the UI, no push notification, and no proof retry UI.

### 4.3 Borrow and repay

**Status: Implemented.**

- Borrow transfers USDC directly to the position owner.
- Repay uses NEP-141 `ft_transfer_call`.
- Interest accrues on borrow, repay, withdraw, and liquidation.
- There is no borrow UI for arbitrary positions, only position `1`.

### 4.4 Withdrawals

**Status: Implemented on-chain; operationally incomplete.**

- Requires a registered Sepolia payout address.
- Requires the amount to exceed the configured gas reserve.
- Requires post-withdrawal health and borrow-limit checks.
- MPC signs an EIP-1559 transaction.
- A relayer broadcasts it.
- Confirmation requires a fresh account proof.

Missing:

- Safe timeout and recovery for signed-but-unbroadcast transactions.
- A UI to broadcast and confirm.
- A user-facing way to inspect the raw transaction.
- A nonce-reconciliation tool.
- A clear process for failed or mismatched proofs.

### 4.5 Position transfer and close

**Status: Partial.**

- Transfer is allowed only when idle, zero debt, no pending operation, and no liquidation ticket.
- Because tickets are never removed, any position that ever liquidated becomes permanently non-transferable.
- `Closed` is an automatic status when debt and collateral are zero; there is no explicit close, storage refund, or cleanup.

---

## 5. Liquidation

### 5.1 What exists

- Health-factor liquidation.
- Partial liquidation up to the debt.
- Fixed penalty.
- Sepolia payout to the liquidator's registered address.
- Ticket lifecycle: `PendingSignature`, `Signed`, `Settled`, `Cancelled`.
- Liquidator agent with unit tests.
- One successful live liquidation.

### 5.2 What is missing

| Gap | Why it matters |
|---|---|
| Refund/claim on failed liquidation | Liquidator USDC can be lost (F-01) |
| Terminal ticket semantics | Later signatures overwrite settled status (F-02) |
| Bad-debt recognition | Debt can be zeroed with an unrecognized shortfall (F-03) |
| Safe retry/recovery | Signed or failed payouts can strand funds (F-04) |
| Close factor | A liquidator can be forced to repay the full debt |
| Auction or Dutch mechanism | First-come fixed-penalty payout can be unfair |
| Stability pool / insurance | No backstop for shortfalls |
| Liquidation UI | Only agents can liquidate |
| Liquidation history/events | No audit trail or user-visible record |
| Grace period/cooldown | No protection against oracle blips |

The current liquidation design is a minimal testnet mechanism, not a production liquidation engine.

---

## 6. Oracle and pricing

**Status: Implemented as a team-operated feed; missing production safeguards.**

Existing:

- Ed25519 signatures over a canonical message.
- Threshold validation.
- Sequence and freshness checks.
- ETH/USD and USDC/USD in 1e8 precision.
- Oracle agent and tests.

Missing:

- Independent fallback oracle.
- DEX TWAP or Chainlink integration.
- Deviation/velocity circuit breaker.
- Duplicate-key rejection.
- Key rotation and signer-set events.
- Slashing or accountability.
- Staleness flag in `get_health`.
- A documented price-source and outage procedure.
- An on-chain emergency price freeze with clear semantics.

---

## 7. Cross-chain and proof infrastructure

### 7.1 What exists

- Per-position derived Sepolia EOA.
- EIP-1186 account-proof verification.
- Sepolia header parsing and MPT verification.
- Light-client `block_hash_safe` check.
- EIP-1559 transaction assembly and secp256k1 recovery.
- Relayer with proof generation, broadcast, and reconciliation.
- Live deposit, borrow, repay, withdrawal, and liquidation evidence.

### 7.2 What is missing

| Gap | Why it matters |
|---|---|
| Multi-provider proof quorum in live operation | Single RPC is a liveness and integrity dependency |
| Storage proofs | Cannot support contract-based or more complex collateral |
| Automatic proof retry with safe state transitions | Stuck pending operations |
| Signed-payout timeout and cancellation | Signed-but-unbroadcast funds can be stranded |
| Nonce reconciliation and replacement policy | Retries can conflict with old transactions |
| Multi-chain support | The "universal" market vision is not implemented |
| Multi-asset collateral | ETH only |
| Multi-asset debt | USDC only |
| Cross-chain liquidation routing | Liquidator payout is Sepolia-only |
| Chain-specific gas/fee policy | One static gas configuration |
| Reorg/replacement tests | Operational risk is under-tested |

---

## 8. Agents and automation

### 8.1 Existing

- `relayer`: proofs, submissions, broadcast, reconcile.
- `oracle`: sign/submit prices.
- `watcher`: monitor positions and pending operations.
- `keeper`: identify liquidatable positions.
- `liquidator`: execute liquidation.
- `shared`: RPC, borsh, risk math, canonical JSON.
- Unit tests for each agent suite.

### 8.2 Missing

- Production scheduler or deployment manifests.
- Metrics, dashboards, and alerting.
- Structured logs and trace correlation.
- Health checks and readiness probes.
- Durable job queue with idempotency keys.
- Automatic retry with bounded backoff and dead-letter handling.
- Key management/HSM integration.
- Separate testnet/mainnet configuration profiles.
- Live end-to-end orchestration tests.
- On-call and incident-response runbooks.
- Rate-limit and provider-failover strategy beyond ad hoc fallback.

---

## 9. Frontend and user experience

**Status: Demo only.**

Existing:

- MyNearWallet testnet connection.
- Hardcoded position `1`.
- Borrow, repay, withdraw request, payout-address registration, USDC storage registration.
- Some real/simulated labels.

Missing:

- Position creation and discovery.
- Deposit address display and deposit status.
- Proof submission and confirmation.
- Withdrawal broadcast and confirmation.
- Liquidation.
- Transaction history.
- Accurate live data instead of stale v1 links.
- Test USDC onboarding.
- Error recovery for pending/review states.
- Clear risk disclosures.
- Responsive/mobile and accessibility work.
- Wallet options beyond MyNearWallet.

---

## 10. Operations, governance, and security

| Missing capability | Current state | Risk |
|---|---|---|
| Multisig owner/upgrade authority | Single accounts in config | Immediate takeover/upgrade risk |
| Timelock | None | No user exit window |
| Admin event log | None | No on-chain audit trail |
| Two-step ownership | None | Irreversible mistakes |
| Parameter bounds | Partial constructor checks | Owner can change gas within checks |
| Emergency shutdown | `risk_off` only | No full pause or recovery mode |
| Key rotation | None | Oracle/MPC key compromise is hard to contain |
| Incident response | None in repo | Ad hoc handling |
| Upgrade migration plan | None | Storage layout changes are risky |
| Monitoring/alerts | Not in repo | Failures may go unnoticed |
| Bug bounty / disclosure policy | None | No external reporting channel |
| Independent audit | None | This is a read-only internal review |
| CI security checks | None | No automated regression gate |

---

## 11. Data, events, and indexing

**Status: Absent.**

- No NEP-297 events.
- No deposit, borrow, repay, withdraw, liquidation, ticket, oracle, or admin events.
- No indexer or subgraph.
- No per-position history; only current state.
- No transaction-to-position index outside ad hoc docs.
- The UI and agents must query and reconcile state manually.

This is a significant gap for a market product: users, risk monitors, and auditors need a durable event history.

---

## 12. Testing and assurance

### Existing

- 36 contract unit tests.
- 2 local E2E tests.
- Relayer 24, watcher 16, keeper 7, liquidator 10, oracle 4.
- `cargo fmt`, clippy, and wasm build previously verified.
- Some real Sepolia proof vectors.

### Missing

- Fuzzing for RLP, MPT, Keccak, and EIP-1559 code.
- Differential tests against a reference implementation.
- Formal verification of proof and accounting invariants.
- Invariant tests for total debt, collateral, and bad debt.
- Failure-path tests for liquidation refunds and ticket terminality.
- Timeout/recovery tests for signed payouts.
- Storage migration tests.
- Multi-position concurrency tests.
- Live orchestration tests for all agents together.
- CI workflows.
- Coverage reporting.
- External security audit.

---

## 13. Product and vision gaps

The long-term vision is a universal cross-chain money market. The current implementation is a narrow credit-line prototype. The largest gaps between the two are:

1. **No supplier market** - there is no way for capital to enter and earn yield.
2. **No loss-absorption mechanism** - bad debt is unrecognized.
3. **No shared liquidity** - each position has isolated debt and collateral.
4. **No multi-asset or multi-chain support** - ETH on Sepolia and USDC on NEAR only.
5. **No production oracle** - team-operated 2-of-3 with no fallback.
6. **No production liquidation engine** - no auction, close factor, insurance, or refund path.
7. **No event/indexing layer** - no durable market history.
8. **No governance/timelock** - admin and upgrade authority are centralized.
9. **No self-serve UI** - onboarding, proof, withdrawal, and liquidation are operator-driven.
10. **No independent audit** - only this read-only review exists.

Adding supplier accounting to the current per-position state layout is not a simple extension. It requires a deliberate market architecture, a loss-allocation policy, and a migration plan.

---

## 14. What is not missing

For balance, these core pieces are present and should not be rebuilt from scratch without a concrete reason:

- per-position Sepolia address derivation via MPC;
- EIP-1186 account-proof verification against a light-client-confirmed header;
- EOA-only collateral checks;
- EIP-1559 transaction assembly and signer recovery;
- health factor, collateral factor, liquidation threshold, and penalty math;
- direct USDC borrow and NEP-141 repay;
- partial liquidation with a ticket;
- proof submission and broadcast tooling;
- monitoring agents and unit tests;
- live Sepolia/NEAR evidence for the happy path.

The right next step is to harden these pieces and add the missing market layer deliberately, not to replace the proof architecture before its failure paths are fixed.
