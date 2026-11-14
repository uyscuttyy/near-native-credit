# Phase 1 Operational Readiness

Date: 2026-11-14
Branch: `phase-1/security-hardening`
Deployment status: not deployed. Live state was not modified.

## Readiness decision

**No production deployment yet.** Phase 1 code and local tests are complete, but the following gates remain:

1. Commit and review the changes.
2. Add and test a one-time private legacy-ticket migration for the existing v2 `Signed` ticket.
3. Snapshot/export live contract state.
4. Build and review the production wasm.
5. Upgrade/redeploy in a controlled window.
6. Reconcile the existing live v2 withdrawal.
7. Run the legacy-ticket migration, then move the guardian off v1.
8. Re-run the live two-provider proof path.

The live v2 ticket is still `Signed`, and the live v1 position is still `NeedsReview`. The code changes do not retroactively repair persisted state.

## Live state as last observed

### v2 position 1

- Status: `WithdrawalPending`
- Collateral: `0.013897999998392 ETH`
- Debt: `0`
- Nonce: `3`
- Liquidation ticket: `Signed`
- Sepolia transaction:
  - hash: `0x2a90b77ad94583af484e0f25fdf3f33a69c4d9e60869fbf7830b86738271f58f`
  - block: `12080980`
  - `gasUsed`: `204600`
  - status: succeeded
- NEAR confirmation: not submitted

### v1 position

- Status: `NeedsReview`
- Requires investigation before any cleanup.

Do not run `clear_stuck_proof` on either position without checking whether a signed payout exists. Signed payouts require proof.

## Upgrade considerations

### Storage

- No serialized fields were added to `Contract`, `Position`, or `LiquidationTicket`.
- Only `TicketStatus` variants were added:
  - `PendingSignature`
  - `Signed`
  - `Settled`
  - `Cancelled`
  - `RefundPending`
  - `Refunding`
  - `Refunded`
- Existing state can be read by the new code without a field migration.
- Old code cannot safely read tickets that use the new refund variants. Do not roll back after new statuses are written unless state is restored from a snapshot.
- There is no state migration function in this phase.

### Initialization requirements

For a fresh deployment, verify:

- `chain_id == 11155111`; the contract rejects any other value.
- Oracle keys are non-empty, unique, and controlled by the intended signers.
- `oracle_threshold` is achievable with the configured keys.
- `gas_limit >= 250_000`.
- `min_withdrawal_wei > max_gas_wei`, where `max_gas_wei = gas_limit * max_fee_per_gas`.
- Owner and guardian are the intended accounts.
- USDC and light-client account IDs are correct.
- `get_config()` returns the expected values before funding.

### Artifact

- `cargo check --target wasm32-unknown-unknown` passed.
- A production wasm build and byte-for-byte review are still required.
- The native-target `cargo check` command is not usable because of near-sdk guidance.

## Runbooks

### 1. Liquidation refund stuck in `RefundPending`

1. Read the position and ticket.
2. Confirm `ticket.status == RefundPending`.
3. Call `refund_liquidation(position_id)`.
4. Watch for `liquidation_refunded` or `liquidation_refund_failed`.
5. If it returns to `RefundPending`, inspect USDC balance/storage and retry.

If the ticket is `Refunding`, wait for the callback. Do not start a second refund attempt.

### 2. Signed withdrawal or liquidation payout

1. Check the pending withdrawal for `raw_tx` and `tx_hash`.
2. Check the Sepolia receipt and nonce.
3. If the transaction landed, generate a finality proof and call `confirm_withdrawal`.
4. If it did not land, do not clear the proof manually.
5. If the transaction is signed but unbroadcast, treat it as a reconciliation incident. The signed raw transaction may still be broadcast by anyone.

`clear_stuck_proof` intentionally rejects signed payouts.

### 3. Unsigned pending operation after a proof mismatch

1. Confirm the position is `NeedsReview`.
2. Confirm the pending withdrawal has no `raw_tx`.
3. Confirm the timeout: `requested_block + 100_000 < current block`.
4. Owner or guardian calls `clear_stuck_proof(position_id)`.
5. If the operation is an active unsigned liquidation, the contract refunds the liquidator.

### 4. Proof provider quorum failure

1. Configure at least two independent Sepolia RPC endpoints.
2. Verify both return `eth_chainId == 0xaa36a7`.
3. Verify both return matching finalized/latest block and account proof data.
4. If one provider is blocked, replace it; do not lower `--min-quorum` below 2.
5. If quorum cannot be reached, stop; do not submit a single-provider proof.

### 5. Chain-id mismatch

1. Query `get_config()`.
2. Require `chain_id == 11155111`.
3. Do not initialize or upgrade with `1115711` or any other value.
4. New initializations reject the mismatch with `invalid chain id`.

### 6. Oracle incident

1. Check `get_price()` for timestamp, sequence, and signer threshold.
2. Future timestamps are rejected by the contract.
3. Stale or future prices make `get_health` return `0`.
4. Rotate keys only through an approved admin process; there is no timelock or rotation helper in this phase.
5. Do not lower the oracle threshold to work around missing signers.

## Operational risks

- Two-provider live proof generation is not yet re-validated.
- Public Sepolia RPCs can rate limit, lag, or reject finalized proofs.
- The oracle remains centralized and team-operated.
- The custom consensus-critical verifier has no fuzzing or formal assurance.
- Admin actions have no timelock.
- Full liquidation leaves a small gas reserve in the position; it is below the minimum withdrawal and may remain indefinitely.
- Terminal liquidation tickets are retained in position state.
- `get_positions_by_owner` remains an unbounded scan.

## Go/no-go checklist

- [ ] Changes committed and reviewed.
- [ ] Legacy-ticket migration implemented and tested.
- [ ] Live state exported and backed up.
- [ ] Production wasm built and reviewed.
- [ ] Deployment/upgrade script prepared and dry-run.
- [ ] Two independent Sepolia providers validated.
- [ ] Live v2 withdrawal reconciled with a finality proof.
- [ ] Live v1 `NeedsReview` investigated.
- [ ] Rollback plan documented, including the new-status incompatibility.
- [ ] Monitoring/alerts configured for refunds, `NeedsReview`, failed payouts, and stale prices.
