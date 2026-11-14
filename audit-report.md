# Native Credit: Audit Report

**Audit date:** 2026-11-14  
**Baseline commit:** `f8241dc` (working tree includes uncommitted deployment and documentation changes)  
**Networks reviewed:** NEAR testnet, Ethereum Sepolia  
**Contract:** `v2.native-credit.testnet`, wasm sha256 `345e1c91175345e78fc5b49c4ea6b9b18f06ed9de7dd3fbb9447264b7f330efd`  
**Review type:** Read-only internal security and architecture audit  
**Status:** Findings open; no source, deployment, or live-confirmation changes made during the audit

---

## 1. Executive summary

Native Credit is a working testnet prototype of an overcollateralized credit line. A NEAR account can derive a Sepolia EOA through the MPC, deposit ETH to it, prove the deposit to a NEAR contract through a light client, borrow testnet USDC, repay, request a signed ETH withdrawal, and have an unhealthy position liquidated. The happy path is implemented and has been exercised on live testnet, including one liquidation.

It is **not** a complete money market and **not production-ready**.

The core design is directionally sound:

- collateral is not credited until `block_hash_safe` confirms the supplied header;
- Sepolia account proofs are verified against the header state root;
- withdrawals require an MPC signature that recovers to the derived address;
- EIP-1559 transactions are assembled and broadcast by a relayer;
- health, borrow-limit, interest, and liquidation math exist and are unit-tested.

The main problems are lifecycle and accounting failures:

1. **A failed liquidation can consume the liquidator's USDC with no refund and no recoverable claim.**
2. **Liquidation ticket status is overwritten by later signatures; the live v2 ticket is currently `Signed` when it should be terminal.**
3. **Bad debt is never recorded; debt can be zeroed even when seized collateral is worth less than the repayment.**
4. **There is no safe recovery path for signed-but-unbroadcast payouts or light-client proof mismatches.**
5. **The live proof run used a single RPC provider, not the intended multi-provider quorum.**

No unauthenticated direct-theft path was found in the normal flow. The High findings are about loss, lock-up, and incorrect accounting under failure conditions. They are release blockers for any third-party or production use.

### Verdict

| Use case | Verdict |
|---|---|
| Controlled testnet demo | Conditionally usable, provided live state is described accurately |
| Third-party testnet users | Not ready |
| Production / mainnet | Not ready |
| Universal cross-chain money market | Not implemented |

---

## 2. Scope and method

### 2.1 In scope

- `contracts/native-credit/src/lib.rs`: contract state, lifecycle, access control, accounting, oracle, liquidation.
- `contracts/native-credit/src/crypto.rs`: RLP, Keccak, MPT account proofs, EOA checks, EIP-1559 assembly, signature recovery.
- `contracts/native-credit/src/math.rs`: USD conversion, health factor, borrow limit, interest, liquidation math.
- `contracts/native-credit/tests/local_e2e.rs`: local lifecycle and liquidation harness.
- `agents/relayer/`, `agents/oracle/`, `agents/watcher/`, `agents/keeper/`, `agents/liquidator/`, `agents/shared/`.
- `web/`: demo UI and wallet integration.
- `README.md`, `HANDOFF.md`, `VERIFY.md`, `AUDIT/`: documentation and prior evidence.
- Live NEAR testnet contract state and Sepolia transaction receipts/balances.

### 2.2 Out of scope

- MPC implementation and key custody.
- Light-client implementation and Ethereum consensus safety.
- Sepolia consensus and deep reorg analysis.
- USDC issuer behavior.
- MyNearWallet and browser security.
- Off-chain host, CI, secret, and RPC account security.
- Economic design, legal characterization, and compliance.
- Independent cryptographic proof or formal verification.

### 2.3 Method

1. Read the contract and crypto/math modules end to end.
2. Trace every entry point and callback, including `#[private]` callbacks.
3. Review tests and run the available suites.
4. Query live NEAR RPC for config, price, positions, and token balances.
5. Verify Sepolia receipts and balances for the live transactions.
6. Compare documentation claims with code and live state.
7. Classify each capability as implemented, partial, or absent.

### 2.4 Constraints

- No code, deployment, or live confirmation was performed.
- The worktree is dirty; the audit snapshot includes uncommitted changes.
- Live state is time-sensitive and was captured on 2026-11-14.
- Public RPC access is read-only and may be rate-limited.

---

## 3. Evidence summary

### 3.1 Source review

Key contract references:

| Area | Reference |
|---|---|
| Config and validation | `lib.rs` L33-64, L174-224 |
| Position model | `lib.rs` L143-156 |
| Position creation/derivation | L226-301 |
| Address registration | L302-307 |
| Deposit proof | L308-343 |
| Borrow | L344-392 |
| Withdraw request | L413-485 |
| Withdraw confirmation | L486-532 |
| MPC signature callback | L533-590 |
| Light-client callback | L591-683 |
| Repay/liquidate callback | L684-787 |
| Oracle update | L788-833 |
| Admin controls | L834-862 |
| Stuck proof clearing | L864-887 |
| Views | L888-936 |
| MPC/light-client call helpers | L938-986 |
| Liquidation withdrawal | L987-1018 |
| Withdrawal cancellation | L1019-1027 |
| Interest accrual | L1057-1081 |

Crypto references:

| Area | Reference |
|---|---|
| Keccak | `crypto.rs` L26 |
| Header parsing | L54 |
| Account proof | L90 |
| Account decoding/EOA check | L106 |
| MPT proof verification | L166 |
| Node verification | L179 |
| Derived pubkey to address | L302 |
| Signer recovery | L339, L371 |
| EIP-1559 signing hash | L418 |

### 3.2 Tests

| Suite | Result |
|---|---|
| Contract unit tests | 36 passed |
| Contract local E2E | 2 passed |
| Relayer | 24 passed |
| Watcher | 16 passed |
| Keeper | 7 passed |
| Liquidator | 10 passed |
| Oracle | 4 passed |
| Web | No test suite |

Previously verified: `cargo fmt --check`, clippy, and wasm release build. No CI workflow files were found.

### 3.3 Live state

**v2 contract**

| Item | Value |
|---|---|
| Account | `v2.native-credit.testnet` |
| Wasm size | `609491` bytes |
| Code hash | `4XRS7nEZJqD6JC5zbE35xMn2gG1D1PVi7yZrChHjxx3N` |
| Deploy tx | `AoaLM6MDSS2nsziqgmmgwhmjpAA65KdmMGCv7gYGBoJT` |
| Owner | `v2.native-credit.testnet` |
| Guardian | `native-credit.testnet` |
| Chain id | `11155111` |
| Gas limit | `250000` |
| Fees | max `2000000000`, priority `1000000000` |
| Oracle | 3 Ed25519 keys, threshold `2` |
| USDC balance | `20000000` (20 USDC) |
| Price | ETH `$3000`, USDC `$1`, sequence `7`, timestamp `1794021815` |

**v2 position 1**

| Item | Value |
|---|---|
| Owner | `borrower.native-credit.testnet` |
| Deposit address | `0x81163cfa3c95b3691fdb4fb0766ecafcfa267999` |
| Status | `WithdrawalPending` |
| Collateral | `13897999998392000` wei (`0.013897999998392 ETH`) |
| Debt | `0` USDC |
| Last verified block | `12080892` |
| Last verified nonce | `3` |
| Pending withdrawal | `0.01 ETH` to `0x0d5d9acdb6adf6dce00d14972f7ce7b1e99b8fa6`, nonce `3`, `max_gas_wei=500000000000000` |
| Sepolia tx | `0x2a90b77ad94583af484e0f25fdf3f33a69c4d9e60869fbf7830b86738271f58f` |
| Sepolia receipt | block `12080980`, status `0x1`, `gasUsed=204600` |
| NEAR confirmation | Not submitted |
| Liquidation ticket | `Signed`; liquidator `borrower.native-credit.testnet`; collateral `0.036 ETH`; repay `10 USDC`; address `0xec29...89bc` |

**v1 contract/position**

| Item | Value |
|---|---|
| Account | `native-credit.testnet` |
| Deposit address | `0xec29df4cf529cf8be4fbc8380cf732cff82389bc` |
| Status | `NeedsReview` |
| Collateral | `49966999999448000` wei (`0.049966999999448 ETH`) |
| Debt | `0` USDC |
| Last verified block | `12080729` |
| Last verified nonce | `1` |
| Pending withdrawal | `0.01 ETH` to v2 deposit address, nonce `2` |
| Sepolia tx | `0xf5dc0ee8f56a6899c28d752fc9c64726e448aceb2063128db78e865698ff5fb0` |
| NEAR confirmation | `7wrKoqyPqq5GGJbyQykknz7Nu9Tf9YhJaYMGrQtcTuZf` |

**Balances**

| Address | Balance |
|---|---|
| v2 deposit `0x811...` | `3693399995323000` wei (`0.003693399995323 ETH`) |
| v1 deposit `0xec29...` | `85945999999049000` wei (`0.08594599999904899 ETH`) |

**Live transaction trail**

| Action | Evidence |
|---|---|
| Deposit proof | `9MfPsqDmXfHPkNxbxNsHAkQ3b9PXRBDtW3VgbL1sJL5d` |
| Borrow | `8UJixg1jFukkEPp93KzgG6Vu1vEDfE98vVV7jeNsBdjp` |
| Repay | `B5BaDvaqNths4BRD4hjnYagFseyR3jjGqT21N3U6yP2w` |
| Failed fresh withdrawal | Sepolia `0x66a3a1e2e4cb7a5d70ec55ac10a0fea97255bf077035a04ef2396c70fd7fc74d`, block `12080523`, `status=0x0`, `gasUsed=60000` |
| Failed confirmation | `EWTC7kyUXzcjC4WdjBmBNKJUXjTXYvf69ZnqBpcRbk83` |
| Existing-EOA withdrawal | Sepolia `0x0ce9ccdf7abacfd77b4d4569e46ea3dcefb512034a66d68b4e180b42d2d4ca40`, block `12080627`, `gasUsed=21000` |
| Existing-EOA confirmation | `8aSvnnnJ8NroiUUr6WXhiUgSCoGhrrK5SsUL9iHKDWHK` |
| Liquidation payout | Sepolia `0x979441e8c1ea57fefa77743b223332eb9c2a3ec9895de84f648a28c9124f8109`, block `12080719`, `gasUsed=21000` |
| Liquidation call | `22ckDtWYQisemyHZbhRsUj4AxX9TKfPbfaHQVXAY9p8v` |
| Liquidation confirmation | `CZtEnZMN5GEYPvMNzqayU8bHUSVPZ4wvd8Db4BiPPMAQ` |
| Fresh payout retry | Sepolia `0x2a90...f58f`, block `12080980`, success, `gasUsed=204600` |
| Fresh payout NEAR confirmation | Pending audit approval |

### 3.4 Gas finding

Sepolia's EIP-8037 state gas makes a transfer to a fresh EOA cost about `207391` gas:

- `183600` state gas;
- `21000` intrinsic execution gas.

The original v2 `60000` gas limit failed at `gasUsed=60000`. Existing-EOA transfers still cost `21000`. Live v2 is now upgraded to `250000`, and the fresh payout retry used `204600`, below the limit.

The source comment at `lib.rs` L20 still says the deployed v2 uses 60,000. That is stale.

### 3.5 Proof quorum

The intended model is multi-provider proof agreement. The live path used `--min-quorum 1` because thirdweb returned HTTP 403 and several other public providers rejected pinned-finalized `eth_getProof`. `VERIFY.md` documents this as a workaround, not the production trust model. The live run therefore did not satisfy the intended quorum.

---

## 4. Component scorecard

| Component | Classification | Assessment |
|---|---|---|
| Position creation and MPC address derivation | Implemented and verified | Live position derived and used |
| Sepolia deposit proof | Implemented and verified | Live deposit proof and credit |
| EOA-only check | Implemented | Account proof requires plain EOA |
| Borrow | Implemented and verified | Live borrow and repay |
| Repay | Implemented and verified | NEP-141 `ft_transfer_call` |
| Health/borrow math | Implemented, under-tested | Unit tests; no invariant tests |
| Withdrawal request/sign/broadcast | Implemented and verified | Live user withdrawal |
| Withdrawal confirmation | Implemented and verified | Existing-EOA and fresh-EOA paths |
| Liquidation happy path | Implemented and verified | Live liquidation settled once |
| Liquidation failure/refund | Absent/broken | No refund or claim |
| Bad-debt accounting | Absent | `total_bad_debt_usdc` unused |
| Oracle | Implemented, team-operated | 2-of-3 Ed25519; no fallback |
| Proof quorum | Partial | Live run used one provider |
| Custom proof verifier | Implemented, under-assured | No fuzz/formal/differential |
| Admin/governance | Partial | Owner/guardian, no timelock/events |
| Events/indexing | Absent | No event log |
| Frontend | Partial demo | Hardcoded position 1, stale evidence |
| Supplier market | Absent | No LP shares, pool, yield |
| Multi-asset/multi-chain | Absent | ETH/Sepolia and USDC/NEAR only |
| Tests | Implemented, with gaps | 38 contract tests; no failure-path coverage |
| Documentation | Stale | Live state contradicts README/HANDOFF |

---

## 5. Top findings

| ID | Severity | Finding | Impact |
|---|---|---|---|
| F-01 | High | Failed/stuck liquidation consumes liquidator USDC | Liquidator loses funds with no refund/claim |
| F-02 | High | Ticket status overwritten by later signatures | Live ticket `Signed`; transfer blocked; accounting wrong |
| F-03 | High | Bad debt never recorded | Debt zeroed while the protocol is economically short |
| F-04 | High | No safe recovery for signed/unbroadcast or mismatched proofs | Funds/state can be stuck or desynced |
| F-05 | Medium | Oracle centralization/duplicate keys/future timestamps/stale health | Misprice and single-key threshold risk |
| F-06 | Medium | Single-provider proof quorum in live run | RPC integrity/liveness dependency |
| F-07 | Medium | Custom proof/transaction verifier under-assured | Consensus-critical code lacks fuzz/formal review |
| F-08 | Medium | Admin centralization, no timelock/events | Immediate parameter/upgrade risk; no audit trail |
| F-09 | Medium | Ticket lifecycle blocks transfer and lacks cleanup | Positions become permanently non-transferable |
| F-10 | Low | Unbounded owner position scan | Scale and gas/RPC limits |
| F-11 | Low | Fixed storage deposit/no cleanup | Over/underpayment and state bloat |
| F-12 | Low | Demo UI stale/incomplete | Misleading evidence; no self-serve lifecycle |
| F-13 | Info | No supplier market/economic model | Current product is not the stated money market |

Detailed evidence and recommendations are in `security-review.md`.

---

## 6. Feature status

### Implemented and exercised

- Per-position MPC-derived Sepolia address.
- EIP-1186 account proof verification against a light-client-confirmed header.
- EOA-only collateral check.
- ETH deposit credit.
- USDC borrow and repay.
- Interest accrual.
- Health factor and borrow limit.
- User withdrawal request, MPC signature, broadcast, and confirmation.
- Liquidation repay, collateral seize, Sepolia payout, ticket.
- Team-operated Ed25519 price quorum.
- Relayer, watcher, keeper, liquidator, oracle agents.

### Partial

- Position transfer: blocked after any ticket.
- Position close: automatic status only; no cleanup/refund.
- Proof quorum: implemented but live run used one provider.
- Recovery: no safe timeout/cancel for signed payouts.
- Liquidation failure handling: ticket cancellation without refund.
- Frontend: demo for position 1 only.
- Governance: owner/guardian controls with no timelock/events.
- Tests: happy paths and some failure checks; high-risk failure paths untested.

### Absent

- Supplier deposits, LP shares, lender withdrawals.
- Utilization-based interest and lender yield.
- Reserves, protocol fees, treasury.
- Bad-debt recognition/socialization.
- Debt ceilings and market caps.
- Close factor, auction, stability pool, insurance.
- Independent oracle fallback, key rotation, slashing.
- Storage proofs.
- Multi-asset and multi-chain support.
- NEP-297 events/indexer/history.
- Self-serve UI for create/deposit/confirm/liquidate.
- CI, fuzzing, formal verification, bug bounty, independent audit.
- Production monitoring, incident response, timelock, multisig upgrade process.

See `missing-features.md` for the full matrix.

---

## 7. Documentation discrepancies

The audit found material drift between docs, code, and live state.

| Claim | Reality |
|---|---|
| `README.md`/`HANDOFF.md`: v2 is `Idle` | Live v2 is `WithdrawalPending` |
| Docs: v2 collateral `0.003897999998392 ETH` | Live v2 collateral `0.013897999998392 ETH` |
| Docs: v2 ticket `Settled` | Live v2 ticket `Signed` |
| Docs: final fresh withdrawal pending a top-up | Sepolia payout already succeeded at block `12080980`; NEAR confirmation is pending |
| Docs: final state `last_verified_block=12080729` | Live `12080892`; nonce `3` |
| Docs: v1 not prominent | v1 is `NeedsReview`, with a pending withdrawal |
| `lib.rs` L20: deployed v2 still uses 60,000 | Live v2 uses 250,000 |
| Docs/prior audit: baseline tag | No tag found; only commit `f8241dc`, untracked files present |
| `VERIFY.md`: 2-provider intended model | Live run used `--min-quorum 1` (documented as a workaround) |
| Prior `AUDIT/AUDIT.md`: test counts/chain-id finding | Not reproduced as current; use this report's evidence |

The frontend also contains stale v1 transaction evidence and a hardcoded withdrawal link. `README.md` is closer to current than `HANDOFF.md`, but both need a full live-state rewrite.

Do not silently "fix" these claims by changing docs to match a desired state. Reconcile the live chain first, then write the docs to match verified state.

---

## 8. Release readiness

### Controlled testnet demo

**Conditional.** The happy path works and the evidence is real. Before a demo:

- state clearly that v2 is `WithdrawalPending`, not `Idle`;
- state that the ticket is `Signed`, not `Settled`;
- state that the Sepolia payout succeeded but NEAR confirmation is pending;
- state that v1 is `NeedsReview`;
- do not imply the proof quorum is decentralized;
- do not imply there is a supplier market.

### Third-party testnet users

**Not ready.** F-01 through F-04 can cause loss or stuck funds. The UI cannot support onboarding or recovery. The docs are stale. A user could deposit and borrow, but failure paths are not safe.

### Production

**Not ready.** Beyond the High findings, production requires:

- supplier/lender accounting and a loss waterfall;
- independent oracle/fallback;
- multi-provider proof quorum;
- events/indexing;
- multisig, timelock, and upgrade controls;
- external audit, fuzzing, and formal analysis of critical invariants;
- operational monitoring and incident response;
- economic and legal review.

### Minimum gate before the next live user-facing action

1. Approve the findings and assign owners.
2. Reconcile v2 and v1 live state.
3. Fix or explicitly guard F-01 through F-04.
4. Require at least two independent proof providers.
5. Add regression and invariant tests for the failure paths.
6. Update docs to verified live state.

---

## 9. Recommended sequence

The detailed plan is in `recommended-roadmap.md`. In short:

1. **P0: Stop the bleeding** - liquidation refund/claim, ticket terminality, bad debt, safe recovery, proof quorum, live reconciliation, truthful docs.
2. **P1: Audit-ready prototype** - oracle hardening, verifier assurance, events/indexer, governance, observability, CI.
3. **P2: Usable testnet market** - supplier market, utilization/IRM, reserves, bad-debt waterfall, liquidation engine, self-serve UI.
4. **P3: Production/universal market** - multi-asset, multi-chain, decentralization, audits, formal verification, compliance.

The current proof/MPC architecture should be preserved and hardened. The missing market layer should be designed deliberately, not bolted onto the existing per-position accounting.

---

## 10. Limitations of this review

- This is a read-only internal review, not a formal independent audit.
- No formal verification or cryptographic proof was performed.
- The custom verifier was reviewed manually and exercised by existing tests, not fuzzed.
- The worktree is dirty; results reflect the snapshot, not a clean tagged release.
- Live state is time-sensitive and may change after 2026-11-14.
- Public RPC access is not a full archival or adversarial environment.
- Agent suites were run; no live multi-agent orchestration was performed.
- The web UI has no tests and was reviewed statically.
- Economic, legal, and compliance risks were not modeled.
- Out-of-scope components (MPC, light client, USDC, Sepolia consensus) were trusted as described.

---

## 11. Audit artifacts

- `audit-report.md` - this report.
- `current-architecture.md` - system design, data model, flows, live snapshot.
- `security-review.md` - threat model, findings, invariants, test gaps.
- `missing-features.md` - capability matrix and product gaps.
- `recommended-roadmap.md` - phased remediation and market roadmap.

No source files were modified by this audit. The five markdown files above are the only intended additions.
