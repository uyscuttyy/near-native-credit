# Phase 1 Security Fixes

Date: 2026-11-14
Scope: `contracts/native-credit/`, `agents/relayer/`, `agents/shared/`, `agents/liquidator/`, `agents/keeper/`
Deployment status: not deployed; live state unchanged.

## Summary

| ID | Finding | Code status | Live status |
|---|---|---|---|
| F-01 | Failed/stuck liquidation consumes liquidator USDC without refund | Fixed in contract and agent estimates | Not deployed |
| F-02 | Liquidation ticket status is overwritten by later signatures | Fixed in contract | Live v2 ticket still `Signed` |
| F-03 | Bad debt is never recorded | Fixed in contract | Not deployed |
| F-04 | No safe recovery for signed/unbroadcast or proof-mismatched operations | Fixed in contract | Live v1 recovered and retired |
| F-05 | Oracle duplicate keys, future timestamps, stale health | Duplicate/future/stale fixed; centralization and fallback remain | Not deployed |
| F-06 | Live proof path used single-provider quorum | Code now requires two chain-valid providers | Live two-provider run not re-executed |

## F-01: Failed liquidation refunds

### Root cause

`ft_on_transfer` accepted a liquidation, reduced debt immediately, and created a payout. If signing or the Sepolia payout failed, the liquidator's USDC had already been consumed and the debt had already been reduced.

### Fix

- Liquidation requests no longer reduce `position.debt_usdc` or `total_debt_usdc`.
- The accepted amount is capped by collateral value:
  - `math::max_liquidation_repay_usd`;
  - `math::token_value`.
- A failed signature or failed payout calls `prepare_liquidation_refund`.
- `request_liquidation_refund` calls USDC `ft_transfer` with `on_liquidation_refund`.
- Refund states:
  - `Refunding`: transfer in flight;
  - `Refunded`: transfer succeeded;
  - `RefundPending`: transfer failed and can be retried.
- `refund_liquidation(position_id)` is public and permissionless while the ticket is `RefundPending`.
- `cancel_withdrawal`, `on_sign` failure, `on_block_hash` payout failure, and `clear_stuck_proof` all route active liquidation tickets through the refund path.
- E2E coverage verifies failed refund callbacks, permissionless retry, and eventual `Refunded` state.

### Important accounting detail

A Sepolia EOA must retain ETH for gas. Full liquidation therefore seizes `collateral - max_gas_wei`, not the entire balance. The small residual remains in the position. Bad debt is recognized when the seized collateral plus the gas reserve covers the old collateral, meaning the collateral is economically exhausted.

## F-02: Terminal ticket protection

### Root cause

`on_sign` could update a liquidation ticket to `Signed` even when the ticket was already settled, refunded, or cancelled, and user withdrawals could overwrite the live v2 liquidation ticket.

### Fix

- `on_sign` marks a ticket `Signed` only when:
  - `pending.kind == Liquidation`; and
  - `ticket.status == PendingSignature`.
- Any other late signature cancels/refunds the pending operation without mutating a terminal ticket.
- `transfer_position` now blocks only active tickets (`ticket_is_active`) and permits terminal tickets.
- Added `user_signature_does_not_overwrite_terminal_ticket`.

### Live consequence

The live v2 ticket was migrated from `Signed` to `Settled` on 2026-11-14 after the pending withdrawal was confirmed. The one-time private `migrate_legacy_ticket` path accepted only the exact persisted position `1` state, required no pending operation and zero debt, moved no funds, and recorded a one-time storage flag. The inert path has been removed from source and the cleanup wasm was deployed to v2.

## F-03: Bad-debt recording

### Root cause

`LiquidationTicket` had no bad-debt field, and settlement never wrote off unbacked debt. `total_debt_usdc` could remain above the sum of recoverable debt.

### Fix

- Debt is reduced only in `settle_liquidation`, after a confirmed successful Sepolia payout.
- `settle_liquidation`:
  - reduces debt by the accepted repayment;
  - writes off remaining debt when collateral is exhausted;
  - increments `total_bad_debt_usdc`;
  - decrements `total_debt_usdc` by accepted repayment plus bad debt;
  - marks the ticket `Settled`.
- Added `local_e2e_underwater_liquidation_records_bad_debt` with:
  - debt `4_800_000`;
  - accepted repayment `4_629_629`;
  - bad debt `170_371`;
  - seized collateral `49_500_000_000_000_000` wei, leaving a `500_000_000_000_000` wei gas reserve.

## F-04: Recovery and reconciliation

### Root cause

A signed-but-unbroadcast payout, or a light-client hash mismatch, could leave the contract unable to safely recover. `clear_stuck_proof` could clear state without proving what happened on Sepolia.

### Fix

- `on_block_hash` mismatch sets `NeedsReview` and preserves pending deposit/withdrawal/ticket state.
- `clear_stuck_proof`:
  - owner/guardian only;
  - rejects signed payouts with `signed payout requires proof`;
  - requires a `100_000` block timeout for unsigned pending operations;
  - refunds unsigned liquidation claims;
  - rejects active tickets that require reconciliation.
- `migrate_legacy_ticket` was a one-time, private, strictly constrained status migration; it is now removed from source and the deployed contract after execution.
- `confirm_withdrawal`:
  - accepts `NeedsReview`;
  - requires a signed pending withdrawal;
  - verifies nonce advancement and account proof;
  - can replace a stale pending proof attempt.
- Callback correlation:
  - `on_block_hash(position_id, header_hash)`;
  - `on_sign(position_id, signing_hash)`;
  - stale callbacks are ignored.

### Operational rule

Never use `clear_stuck_proof` for a signed payout. A signed payout must be proven with `confirm_withdrawal` or left in `NeedsReview` for investigation.

## F-05: Oracle hardening

### Fixed

- Duplicate oracle keys are rejected at initialization.
- Duplicate configured keys are skipped during runtime updates.
- Future timestamps are rejected by `update_price`.
- `get_health` returns `0` for missing, stale, or future prices.

### Remaining

- Oracle keys remain team-operated and centralized.
- No fallback oracle, deviation circuit breaker, or per-signer event was added.
- No key-rotation or timelock mechanism was added.

## F-06: Proof quorum and chain identity

### Fixed

- `agents/relayer/src/providers.mjs` now:
  - requires `minQuorum >= 2`;
  - checks every provider with `eth_chainId`;
  - excludes providers not on `0xaa36a7`;
  - fails closed below quorum.
- `agents/relayer/src/proof.mjs` rejects `--min-quorum 1`.
- Relayer tests cover single-provider rejection and wrong-chain rejection.
- `VERIFY.md`, `HANDOFF.md`, and the relayer README no longer recommend a single-provider workaround.

### Remaining

- A live two-provider proof run was not completed in this phase.
- Provider availability, rate limits, and archive/finalized-block support remain operational risks.
- `--no-check-finality` still exists for explicit debugging; it should not be used for production proofs.

## Additional changes

- Liquidation and keeper estimates now include:
  - collateral-capped max repayment;
  - gas reserve;
  - expected bad debt.
- Relayer and contract tests were updated for callback correlation arguments.
- No serialized fields were added to `Contract`, `Position`, or `LiquidationTicket`; only `TicketStatus` variants were added. This avoids a Borsh field-layout migration but still requires an upgrade/redeploy.

## Residual risks not fixed in Phase 1

- Custom consensus-critical verifier is still not fuzzed or formally verified.
- Admin centralization and lack of timelock remain.
- Liquidation ticket lifecycle still has limited cleanup for terminal records.
- Unbounded `get_positions_by_owner` scan remains.
- Fixed storage deposit and no position cleanup remain.
- No supplier market or economic risk model exists.
