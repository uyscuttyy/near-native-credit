# Native Credit: Security Review

**Audit snapshot:** 2026-11-14  
**Baseline commit:** `f8241dc` (dirty working tree included deployment and documentation changes)  
**Scope:** `contracts/native-credit/`, `agents/`, `web/`, deployment configuration, and live testnet state  
**Method:** Manual source review, unit/E2E test review and execution, live RPC state checks, Sepolia receipt checks, and documentation cross-checking  
**Status:** Findings are open. No code or deployment changes were made.

---

## 1. Executive risk summary

No unauthenticated direct-theft path was found in the normal deposit/borrow/withdraw path. The core proof design is directionally sound: collateral is not credited until the light client confirms the header, and payouts require an MPC signature from the derived address.

The most serious risks are lifecycle and accounting failures, not a missing signature check:

1. **A failed liquidation can consume the liquidator's USDC with no refund and no recoverable claim.**
2. **Liquidation ticket status is overwritten by later signatures; the live v2 ticket is currently `Signed` after it should have been terminal.**
3. **Bad debt is never recorded, and debt can be zeroed even when seized collateral is worth less than the repaid amount.**
4. **There is no safe recovery path for signed-but-unbroadcast payouts or proof mismatches; `clear_stuck_proof` can clear state without reconciling Sepolia.**
5. **The live proof run used a single RPC provider, not the intended multi-provider quorum.**

These are testnet-level defects today because the contract holds only testnet USDC and the positions are controlled by the team. They are release blockers for any production or third-party-user deployment.

### Severity counts

| Severity | Count |
|---|---:|
| Critical | 0 |
| High | 4 |
| Medium | 5 |
| Low | 3 |
| Informational | 1 |

### Finding index

| ID | Severity | Finding | Status |
|---|---|---|---|
| F-01 | High | Failed/stuck liquidation consumes liquidator USDC without refund | Open |
| F-02 | High | Liquidation ticket status is overwritten by later signatures | Open |
| F-03 | High | Bad debt is never recorded or socialized | Open |
| F-04 | High | No safe recovery for signed/unbroadcast or proof-mismatched operations | Open |
| F-05 | Medium | Oracle centralization, duplicate-key acceptance, future timestamps, stale health view | Open |
| F-06 | Medium | Live proof path used single-provider quorum | Open |
| F-07 | Medium | Custom consensus-critical proof/transaction verifier is under-assured | Open |
| F-08 | Medium | Admin centralization, no timelock, no event log | Open |
| F-09 | Medium | Liquidation ticket lifecycle blocks transfer and lacks cleanup | Open |
| F-10 | Low | Unbounded position scan in `get_positions_by_owner` | Open |
| F-11 | Low | Fixed storage deposit with no refund or position cleanup | Open |
| F-12 | Low | Demo UI uses stale v1 evidence and incomplete lifecycle | Open |
| F-13 | Informational | No supplier market or economic risk model | Open |

---

## 2. Assets, actors, and trust assumptions

### 2.1 Assets

- ETH collateral at per-position Sepolia EOAs.
- Testnet USDC held by the credit contract.
- USDC supplied by liquidators during liquidation.
- Availability of borrow, repay, withdraw, and liquidation.
- Integrity of the oracle price.
- Integrity of the MPC-derived addresses and signatures.
- Admin and guardian authority.
- Off-chain relayer, oracle, watcher, keeper, and liquidator operations.

### 2.2 Actors

| Actor | Capability |
|---|---|
| Borrower | Creates a position, deposits ETH, borrows, repays, withdraws, transfers an idle position |
| Liquidator | Repays USDC and receives seized ETH at a registered Sepolia address |
| Owner | Sets guardian and gas limit; can use `clear_stuck_proof`; can upgrade the NEAR account if it controls the account |
| Guardian | Can toggle `risk_off` |
| Oracle signers | Sign price messages; 2-of-3 threshold in the live deployment |
| Relayer | Builds proofs, submits NEAR calls, broadcasts signed Sepolia transactions |
| Sepolia RPC providers | Supply headers, proofs, and receipt data |
| MPC signer | Holds derived keys and signs EIP-1559 transactions |
| Light client | Reports safe Sepolia header hashes |
| USDC contract | NEP-141 token used for borrow, repay, and liquidation |
| Frontend | Convenience layer; not trusted for accounting |

### 2.3 Trust assumptions

- The MPC does not sign transactions that were not requested by the contract.
- The light client correctly reports the canonical safe Sepolia header.
- At least one proof RPC returns a correct finalized account proof.
- The oracle threshold is controlled by honest, independent key holders.
- The owner and guardian are operated honestly and securely.
- The NEAR account upgrade authority is controlled by the same trusted operator set.
- The USDC issuer and Sepolia consensus behave as expected.

These assumptions are not enforced on-chain. The review treats them as operational trust boundaries, not as proven properties.

### 2.4 Out of scope

- MPC implementation and key management.
- Light-client implementation and Ethereum consensus.
- Sepolia consensus and reorg behavior beyond the selected safe block.
- USDC issuer behavior.
- MyNearWallet and browser security.
- Off-chain host, CI, secret, and RPC account security.
- Economic viability, legal characterization, and market design.

---

## 3. Detailed findings

### F-01: Failed or stuck liquidation consumes liquidator USDC without refund

**Severity:** High  
**Status:** Open  
**Components:** `ft_on_transfer` liquidation branch, `request_liquidation_withdrawal`, `on_sign`, `on_block_hash`, `cancel_withdrawal`

**Evidence**

- The liquidation branch of `ft_on_transfer` accepts the liquidator's USDC, reduces the position debt, creates a `LiquidationTicket`, and requests a Sepolia payout (`lib.rs` L724-786).
- If signing fails, `on_sign` calls `cancel_withdrawal`, which sets the ticket to `Cancelled`, clears the pending withdrawal, and returns the position to `Idle` (`lib.rs` L533-590, L1019-1026).
- If the Sepolia payout fails, `on_block_hash` sets the ticket to `Cancelled` and returns the position to `Idle` (`lib.rs` L664-675).
- In both cases, the contract does not refund the liquidator's USDC, does not restore the position debt, and does not create a recoverable claim.
- The only USDC outflow path in the contract is the borrower `ft_transfer` in `borrow`; there is no liquidator refund or sweep function (`lib.rs` L376-391).

**Impact**

A liquidator can repay a position and receive nothing if the MPC signature or Sepolia payout fails. The debt reduction remains final. The liquidator's USDC is effectively trapped in the contract unless the position owner later borrows it, which is not a remedy for the liquidator. This is a direct loss of liquidator funds and a strong disincentive to liquidate.

**Scenario**

1. Position is unhealthy.
2. Liquidator calls `ft_transfer_call` with `{"action":"liquidate","position_id":...}` and USDC.
3. Contract accepts USDC, reduces debt, creates a ticket, and requests a signature.
4. MPC signing fails, or the signed transaction cannot be broadcast, or the payout reverts.
5. Ticket becomes `Cancelled`; position returns to `Idle`; liquidator receives no USDC and no ETH.

**Recommendation**

- Do not reduce debt until the Sepolia payout is proven success, or make the payout claim atomic with the repayment.
- If debt must be reduced first, record a first-class liquidator claim with principal, status, and a refund/retry path.
- Refund USDC on any failure or add a bounded, permissionless retry that preserves the claim.
- Add tests for signing failure, broadcast failure, revert, replacement transaction, and reorg.
- Emit an event for every step of the liquidation claim lifecycle.

---

### F-02: Liquidation ticket status is overwritten by later signatures

**Severity:** High  
**Status:** Open  
**Components:** `on_sign`, `on_block_hash`, live v2 state

**Evidence**

- `on_sign` unconditionally sets `liquidation_ticket.status = Signed` whenever any pending withdrawal is signed (`lib.rs` L584-586). It does not check that the pending withdrawal is a liquidation, that the ticket is still `PendingSignature`, or that the ticket is not already terminal.
- The withdrawal success branch only sets `Settled` when `withdrawal.kind == Liquidation` (`lib.rs` L659-663).
- Therefore a later **user** withdrawal signature can overwrite a `Settled` liquidation ticket with `Signed`.
- Live v2 position `1` currently reports `liquidation_ticket.status = "Signed"` after the liquidation payout had previously settled and a later user withdrawal was signed. This matches the code path.
- `transfer_position` rejects any position with a ticket, even a terminal one (`lib.rs` L264-266).

**Impact**

- The ticket no longer reflects the liquidation's terminal state.
- `transfer_position` is permanently blocked for any position that has ever had a ticket.
- Downstream agents, UI, and audit reconciliation can misclassify a settled liquidation as still in progress.
- A future cancellation path could corrupt an already-settled record because terminality is not enforced.

**Scenario**

1. Liquidation succeeds and the ticket becomes `Settled`.
2. Borrower later requests a normal withdrawal.
3. `on_sign` sets the ticket to `Signed` even though no liquidation is pending.
4. The ticket remains `Signed`; the position is now non-transferable and inconsistent with the settled payout.

**Recommendation**

- Only update the ticket when `pending.kind == WithdrawalKind::Liquidation` and `ticket.status == PendingSignature`.
- Make `Settled` and `Cancelled` terminal; reject or ignore subsequent writes.
- Clear the active ticket after settlement or allow `transfer_position` when the ticket is terminal and debt is zero.
- Add a regression test: settle a liquidation, then sign a user withdrawal, and assert the ticket remains `Settled`.
- Add a status-history field or event log instead of overwriting a single status.

---

### F-03: Bad debt is never recorded or socialized

**Severity:** High  
**Status:** Open  
**Components:** `LiquidationTicket`, `ft_on_transfer` liquidation branch, `math::liquidation_collateral_wei`, `total_bad_debt_usdc`

**Evidence**

- `total_bad_debt_usdc` is initialized to zero (`lib.rs` L220) and never read or written elsewhere.
- The liquidation branch reduces debt by `accepted` regardless of the value of the seized collateral (`lib.rs` L761-780).
- `liquidation_collateral_wei` caps the seized amount at the available collateral (`math.rs` L47-63), so a liquidator can repay more debt than the collateral is worth.
- There is no deficit field, no loss allocation, and no reserve.
- The debt can become zero while the protocol has suffered an economic loss.

**Impact**

- The contract's accounting says the position is healthy/closed while the protocol is economically impaired.
- There is no basis for pricing LP shares, setting reserves, or socializing losses.
- If a supplier market is added later, this accounting cannot support it.
- Liquidators may overpay for insufficient collateral because there is no explicit bad-debt boundary or auction.

**Scenario**

1. Position has 100 USDC debt and ETH collateral worth 60 USDC.
2. Liquidator repays 100 USDC.
3. Contract seizes all 60 USDC-equivalent collateral and sets debt to zero.
4. The 40 USDC shortfall is not recorded anywhere.

**Recommendation**

- Compute `deficit_usdc = debt_after - value_of_seized_collateral` and record it per liquidation.
- Increment `total_bad_debt_usdc` and emit an event.
- Decide and document the loss allocation: reserve, insurance fund, supplier socialization, or protocol treasury.
- Restrict liquidation size or use a close factor so a liquidator cannot be forced into an unbounded deficit.
- Add invariant tests: debt reduction must equal cash received plus recognized bad debt.

---

### F-04: No safe recovery for signed/unbroadcast or proof-mismatched operations

**Severity:** High  
**Status:** Open  
**Components:** `on_block_hash`, `clear_stuck_proof`, `cancel_withdrawal`, pending state

**Evidence**

- If `block_hash_safe` returns a mismatch or fails, `on_block_hash` only clears `pending_deposit` and returns (`lib.rs` L600-608). It does not clear `pending_withdrawal` and does not set `NeedsReview`.
- A position can therefore remain `WithdrawalPending` or `LiquidationPending` with a signed `raw_tx`, no proof, and no retry path.
- `clear_stuck_proof` requires `NeedsReview` (`lib.rs` L867-870), so it cannot clear the hash-mismatch state unless something else sets `NeedsReview`.
- When it does run, `clear_stuck_proof` clears `pending_deposit` and `pending_withdrawal`, sets the position to `Idle`, but does not clear or terminalize `liquidation_ticket`, does not reconcile `last_verified_nonce`, and does not verify whether the Sepolia transaction was actually mined (`lib.rs` L864-887).
- A signed transaction can be broadcast by the relayer at any time; there is no on-chain timeout or cancellation marker.
- A new withdrawal uses `last_verified_nonce`. If the old signed transaction is still pending or later mined, a retry can conflict or double-spend at the chain level.

**Impact**

- User or liquidator funds can be stuck indefinitely.
- State can diverge from Sepolia: debt may be reduced, a ticket may exist, and collateral/nonce may not reflect the actual chain.
- Clearing state without checking the chain can enable a second payout attempt or a borrow against stale collateral.
- There is no operator-independent recovery; the owner is a single point of failure and lacks a complete reconciliation tool.

**Scenario A: signed but not broadcast**

1. User requests withdrawal; MPC signs; raw transaction is stored.
2. Relayer never broadcasts it.
3. Position remains `WithdrawalPending` forever; user cannot borrow or withdraw again; `clear_stuck_proof` cannot run because the status is not `NeedsReview`.

**Scenario B: proof mismatch**

1. User submits `confirm_withdrawal`.
2. Light-client lookup fails or returns a different hash.
3. Contract clears only `pending_deposit`; the signed withdrawal remains, status remains `WithdrawalPending`.
4. No retry path exists except a fresh `confirm_withdrawal` with another proof, which may be impossible if the light client has moved past the block.

**Scenario C: unsafe clear**

1. An operator forces `NeedsReview` and calls `clear_stuck_proof`.
2. Contract clears the pending withdrawal and sets `Idle`.
3. The original signed transaction is later broadcast and mined.
4. The contract's nonce/balance state no longer matches Sepolia; a new withdrawal may reuse the old nonce.

**Recommendation**

- Model payout claims explicitly with states: `Unsigned`, `Signed`, `Broadcast`, `ProvenSuccess`, `ProvenFailure`, `Expired`.
- Require a proof or a chain receipt before clearing a signed payout.
- Add a permissionless timeout that moves the position to a safe review state without losing the claim.
- On any mismatch, set `NeedsReview` and preserve all pending fields.
- Add nonce-reconciliation logic that checks whether the signed transaction hash was mined before allowing a replacement.
- Add tests for broadcast failure, dropped transaction, replacement, reorg, and light-client mismatch.
- Do not rely on a single owner to resolve stuck funds.

---

### F-05: Oracle centralization, duplicate-key acceptance, future timestamps, stale health view

**Severity:** Medium  
**Status:** Open  
**Components:** `new`, `update_price`, `get_health`, `fresh_price`, oracle agent

**Evidence**

- The oracle is a team-operated 2-of-3 Ed25519 feed. There is no independent fallback, DEX TWAP, or circuit breaker.
- `new` checks that keys are Ed25519 and that the threshold is not greater than the key count, but does not reject duplicate keys (`lib.rs` L190-199).
- If the same key is configured at two indices, the same signature can be supplied at both indices and counted twice, satisfying a threshold of 2 with one signer.
- `update_price` accepts timestamps up to 60 seconds in the future (`lib.rs` L797-802). A future timestamp extends the effective freshness window.
- `get_health` reads the stored price without calling `fresh_price` (`lib.rs` L912-935), so a view can report a health factor based on a stale price.
- There is no event for price updates, no key-rotation history, and no slashing.

**Impact**

- A single compromised or duplicated oracle key can satisfy the threshold if configuration is wrong.
- The team can misprice collateral; users and liquidators bear the consequence.
- Stale health views can cause users or agents to make incorrect decisions.
- A future-dated price can remain "fresh" longer than intended.

**Recommendation**

- Reject duplicate oracle keys at initialization and on any rotation.
- Require threshold signatures from distinct keys and record which keys signed.
- Reject future timestamps or tightly bound them to the current block time.
- Make `get_health` return an error or a stale flag when the price is stale.
- Add a fallback oracle and a deviation/velocity circuit breaker.
- Document key custody, rotation, and incident response.
- Emit price-update events with sequence, timestamp, and signer set.

---

### F-06: Live proof path used single-provider quorum

**Severity:** Medium  
**Status:** Open  
**Components:** relayer providers, live verification run

**Evidence**

- `agents/relayer/src/providers.mjs` lists thirdweb and OnFinality as default providers (`providers.mjs` L2-3).
- The intended production model is a multi-provider proof quorum (`VERIFY.md` L93-114; `HANDOFF.md` L254).
- During the live run, thirdweb returned HTTP 403 for proof generation; publicnode, tenderly, and 1rpc rejected pinned-finalized `eth_getProof` requests.
- The live run used OnFinality with `--min-quorum 1` as a workaround (`VERIFY.md` L99-114).
- Single-provider quorum is explicitly documented as a live workaround, not the production trust model.

**Impact**

- A single RPC provider can withhold or supply an incorrect proof.
- A malicious or buggy provider can cause failed or delayed finalization, and in the worst case influence which chain state is presented, subject to light-client checks.
- The system's liveness depends on one public endpoint.

**Recommendation**

- Require at least two independent providers for production proofs, with agreement on block hash and account proof.
- Add a local/archive node or a paid provider with reliable finalized `eth_getProof`.
- Pin the block hash, not just the block number, and compare provider responses.
- Fail closed and alert when quorum is not met.
- Do not ship `--min-quorum 1` as a default.

---

### F-07: Custom consensus-critical verifier is under-assured

**Severity:** Medium  
**Status:** Open  
**Components:** `crypto.rs`, proof verification, EIP-1559 signing

**Evidence**

- The contract implements RLP parsing, Keccak, MPT proof verification, EOA detection, EIP-1559 signing hash, and secp256k1 recovery in a custom module (`crypto.rs`; key functions include `keccak256` L26, `parse_header_fields` L54, `verify_account_proof` L90, `decode_account` L106, `verify_trie_proof` L166, `verify_node` L179, `address_from_derived_pubkey` L302, `recover_signer` L339, `recover_address` L371, `eip1559_signing_hash` L418).
- The verifier supports account proofs only, not arbitrary storage proofs.
- Tests cover real proof vectors, tampered proofs, and signing behavior, but there is no fuzzing, property testing, differential testing against a reference implementation, or formal verification.
- The verifier is consensus-critical: a bug can accept an invalid proof or reject a valid one.

**Impact**

- A verifier bug could allow incorrect collateral accounting or block legitimate withdrawals.
- The current assurance is weaker than the value the contract is intended to secure.
- Future contract-based collateral or multi-asset support would require storage proofs and a broader verifier surface.

**Recommendation**

- Add fuzzing for RLP, MPT nodes, account decoding, and EIP-1559 encoding.
- Differential-test against a mature library or an independent implementation.
- Add property tests for proof soundness and completeness on valid/invalid vectors.
- Consider using an audited proof verifier or a light-client API that returns verified account state.
- Document the exact proof assumptions and rejected edge cases.
- Commission an independent review before production.

---

### F-08: Admin centralization, no timelock, no event log

**Severity:** Medium  
**Status:** Open  
**Components:** `new`, `set_guardian`, `set_gas_limit`, `set_risk_off`, `clear_stuck_proof`, deployment

**Evidence**

- `config.owner` is set to the deployer/predecessor and can set the guardian and gas limit, and call `clear_stuck_proof` (`lib.rs` L174-224, L843-887).
- The guardian can toggle `risk_off` (`lib.rs` L834-841).
- There is no timelock, no two-step ownership transfer, no multisig requirement encoded on-chain, and no event emission for admin changes.
- The NEAR account can be upgraded; the upgrade authority is external to the contract.
- The source comment at L20 says the deployed v2 still uses 60,000 gas, but the live config is 250,000, showing configuration/documentation drift.

**Impact**

- A compromised owner or upgrade authority can change critical parameters or replace code immediately.
- Users cannot observe admin changes on-chain without querying state.
- The guardian can block new borrowing and withdrawals but cannot pause repayments or liquidations; that asymmetry may be intended but is not documented as an incident-response policy.

**Recommendation**

- Use a multisig for owner and upgrade authority.
- Add two-step ownership/guardian transfer and a timelock for parameter changes.
- Emit events for every admin action.
- Bound parameter changes and add circuit breakers.
- Document the upgrade and incident-response process, including who can pause what.
- Remove stale deployment comments or generate config docs from live state.

---

### F-09: Liquidation ticket lifecycle blocks transfer and lacks cleanup

**Severity:** Medium  
**Status:** Open  
**Components:** `transfer_position`, `LiquidationTicket`, `on_block_hash`

**Evidence**

- `transfer_position` requires `position.liquidation_ticket.is_none()` (`lib.rs` L264-266).
- A ticket is created on liquidation and is never removed; success sets `Settled`, failure sets `Cancelled`.
- There is no cleanup function and no ticket history.
- The live v2 position has a ticket and is therefore non-transferable even after the liquidation is complete.

**Impact**

- A position can never be transferred after any liquidation, including a successful one.
- The single ticket field conflates active work, history, and terminal state.
- No event or history makes reconciliation difficult.

**Recommendation**

- Separate active liquidation state from historical tickets.
- Clear the active ticket after terminal settlement, or permit transfer when debt is zero and the ticket is terminal.
- Add a ticket history or event log.
- Add tests for transfer before, during, and after liquidation.

---

### F-10: Unbounded position scan in `get_positions_by_owner`

**Severity:** Low  
**Status:** Open  
**Components:** `get_positions_by_owner`

**Evidence**

- The view iterates from `1` to `next_position_id` and clones every matching position (`lib.rs` L896-906).
- Positions are never removed; `next_position_id` only increases.

**Impact**

- The view becomes slower and more expensive as the protocol grows.
- It can exceed RPC or gas limits if used by agents or the frontend.
- It is not suitable for production-scale discovery.

**Recommendation**

- Maintain a per-owner index or an iterable map.
- Paginate results with a cursor.
- Add a bounded scan with an explicit limit.

---

### F-11: Fixed storage deposit with no refund or position cleanup

**Severity:** Low  
**Status:** Open  
**Components:** `create_position`, storage lifecycle

**Evidence**

- `create_position` requires `STORAGE_DEPOSIT_YOCTO` (0.02 NEAR) but there is no refund on close and no storage cleanup (`lib.rs` L13, L226-231).
- `Closed` positions remain in `LookupMap`; there is no `remove` path.
- The deposit is fixed rather than measured against actual storage.

**Impact**

- Users overpay or underpay relative to actual storage.
- Closed positions bloat state and the owner scan.
- No clean accounting for storage deposits.

**Recommendation**

- Measure storage and refund the surplus on close.
- Add an explicit close/cleanup path.
- Remove closed positions from owner indexes.

---

### F-12: Demo UI uses stale v1 evidence and incomplete lifecycle

**Severity:** Low  
**Status:** Open  
**Components:** `web/src/App.jsx`, `web/src/near.js`, `web/.env.example`

**Evidence**

- The UI defaults to v2 but contains stale v1 copy and a hardcoded v1 withdrawal transaction.
- It manages hardcoded position `1`.
- It cannot create or discover positions, verify deposits, broadcast/confirm withdrawals, or liquidate.
- It does not obtain test USDC.
- Live docs and UI disagree about whether the final withdrawal is pending, succeeded, or confirmed.

**Impact**

- A user can be shown incorrect transaction evidence.
- The UI cannot support self-serve onboarding or recovery.
- It is a demo, but the stale data can mislead reviewers and users.

**Recommendation**

- Generate links and status from live contract state and receipts.
- Add position creation, discovery, deposit-proof status, withdrawal broadcast/confirm, and liquidation flows.
- Add network/contract guards and clear testnet labeling.
- Add UI tests for stale-data regression.

---

### F-13: No supplier market or economic risk model

**Severity:** Informational  
**Status:** Open  
**Components:** product architecture

**Evidence**

- There are no supplier deposits, LP shares, utilization curves, reserves, interest distribution, or bad-debt socialization.
- The contract holds a fixed testnet USDC balance (live v2 holds 20 USDC).
- `total_bad_debt_usdc` is unused.

**Impact**

- The current system is a credit-line prototype, not a universal money market.
- Adding supplier accounting to the current state layout would require a migration and a new risk model.
- There is no economic mechanism to price liquidity or absorb losses.

**Recommendation**

- Treat the current contract as a credit-line engine and design the supplier market as a separate layer or a deliberate v2 architecture.
- Do not bolt LP shares onto the existing per-position accounting without a migration plan.
- Build the loss-allocation and reserve model before accepting third-party liquidity.

---

## 4. Invariant review

| Invariant | Expected | Current assessment |
|---|---|---|
| I-1 | `collateral_wei` equals the last verified Sepolia balance | Holds on successful proofs; can diverge after `NeedsReview` or `clear_stuck_proof` |
| I-2 | Debt never exceeds the borrow limit for new risk | Holds at borrow/withdraw time; interest can push an existing position over |
| I-3 | `total_debt_usdc` equals the sum of position debt | Holds on normal paths; no invariant test |
| I-4 | `total_collateral_wei` equals the sum of position collateral | Holds on normal paths; no invariant test |
| I-5 | Every accepted USDC repayment reduces debt or creates a recoverable claim | Broken by F-01/F-03 |
| I-6 | Every liquidation ends in a terminal ticket and a settled claim | Broken by F-02/F-04/F-09 |
| I-7 | A payout cannot be retried with a stale nonce | Not enforced by F-04 |
| I-8 | Health-changing actions use a fresh price | Holds for borrow/withdraw/liquidate; `get_health` is an exception |
| I-9 | A position with debt cannot be transferred | Holds |
| I-10 | A position with a terminal ticket can be transferred | Broken by F-09 |

---

## 5. Security test gaps

The existing tests are useful but do not cover the highest-risk failure modes.

Missing or insufficient tests:

1. Liquidation signing failure with USDC refund/claim assertion.
2. Liquidation broadcast failure and Sepolia revert.
3. Signed-but-unbroadcast withdrawal timeout and recovery.
4. `block_hash_safe` mismatch after a signed withdrawal.
5. `clear_stuck_proof` with a live liquidation ticket and stale nonce.
6. User withdrawal after a settled liquidation ticket.
7. Duplicate oracle keys and duplicate signatures.
8. Future-dated price freshness.
9. `get_health` with a stale price.
10. `transfer_position` after settled/cancelled tickets.
11. Fuzzing and differential tests for `crypto.rs`.
12. Invariant tests for `total_debt_usdc`, `total_collateral_wei`, and `total_bad_debt_usdc`.
13. Storage migration/upgrade tests.
14. Multi-position and per-owner scan limits.
15. Event-log assertions once events are added.

---

## 6. Open questions

1. Is the live v2 ticket being `Signed` an accepted known defect, or should live state be repaired after the audit?
2. Who owns the 20 USDC held by the v2 contract, and is it intended as protocol-owned test liquidity?
3. What is the intended supplier/lender mechanism, and does it require a new contract rather than an upgrade?
4. What is the approved manual procedure for `NeedsReview` positions, especially v1?
5. Are the three live oracle keys distinct and independently custodied?
6. Is the NEAR account upgrade authority a multisig or a single key?
7. Is single-provider proof quorum acceptable for testnet only, or is it already used for user-facing demos?
8. What is the intended loss-allocation policy for bad debt?
9. Is the v1 position meant to be abandoned, migrated, or reconciled?
10. What monitoring and alerting exist outside this repository?

---

## 7. Release assessment

**Testnet demo:** Conditionally usable for controlled demonstrations with known positions and team-operated keys. The live state must be explained accurately; the README/HANDOFF claims of `Idle`/`Settled` no longer match live v2.

**Third-party testnet users:** Not ready. F-01, F-02, F-03, and F-04 can cause loss or stuck funds even with test assets.

**Production:** Not ready. In addition to the high findings, the system lacks supplier accounting, bad-debt handling, a decentralized oracle, multi-provider proof quorum, timelocks, events, and an independent security audit.

The minimum bar before calling the current design "safe for third-party testnet use" is:

- fix or explicitly guard the liquidation refund and ticket terminality bugs;
- define and implement a safe recovery path for signed/unbroadcast payouts;
- record bad debt;
- run proofs with at least two independent providers;
- add regression and invariant tests for every failure path above;
- publish an accurate live-state and incident-response runbook.
