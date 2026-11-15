# Phase 1 Operational Readiness

Date: 2026-11-14
Branch: `phase-1/security-hardening`
Deployment status: deployed to `v2.native-credit.testnet` on 2026-11-14. No mainnet deployment.

## Readiness decision

**Testnet reconciliation complete; not mainnet-ready.** Phase 1 code, local tests, the v2 upgrade, the pending withdrawal confirmation, the guardian change, and the legacy-ticket migration are complete. The following gates remain:

1. Configure operational delivery for the existing watcher alerts (webhook/ops channel).
2. Review remaining Medium/Low findings for Phase 2.

The live v2 ticket is `Settled`, the v1 position is recovered to `Idle`, and v2's guardian is `v2.native-credit.testnet`.

## Live state as last observed

### v2 position 1

- Status: `Idle`
- Collateral: `0.003693399995323 ETH`
- Debt: `0`
- Nonce: `4`
- Last verified block: `12133272`
- Pending operations: none
- Liquidation ticket: `Settled`
- Guardian: `v2.native-credit.testnet`
- Code hash: `GqCZxy7v4eZp1HH8sdqpMgpbENxUVtMscabZQp4oEE6f`
- Deploy tx: `2r6oSSgJpxaHMkNNfcpX2iELZswh8AGub2Npy4kQh8X7`
- Sepolia payout:
  - hash: `0x2a90b77ad94583af484e0f25fdf3f33a69c4d9e60869fbf7830b86738271f58f`
  - block: `12080980`
  - `gasUsed`: `204600`
  - status: succeeded
- NEAR confirmation: `CWHvU2SN8RntCU4srqdY4GBjCxLCf8W7fpRro7xjR4fc`
- Guardian tx: `BEh7YSTg1rLa6tCUH3YavfFj9znLHYZC67JJE9yRBPg8`
- Migration tx: `45XzfjBwYcaAWHRrqoEJ8s5yGYmrKt32SDPQPfmbM3Yz`
- Live two-provider proof: finalized block `12133303`, quorum `2/2`

### v1 position recovered

- Status: `Idle`
- Collateral: `0.000020999999664 ETH` gas dust
- Debt: `0`
- Nonce: `4`
- Last verified block: `12133272`
- Pending operations: none
- Recovered: `0.085904 ETH` to `0x0d5d9acdb6adf6dce00d14972f7ce7b1e99b8fa6`
- Sepolia tx: `0x353b6195c17b93f829cef096e6b5dbd77c3418e43f9da1dafad2796a8aa11a3b` (block `12133272`, status succeeded, `gasUsed=21000`)
- NEAR txs:
  - clear: `8JPE32e1xHbdB3ss4HBJh9UWTVMdzaLVHDN2DGQfVGFh`
  - record deposit: `AtJzmp3LMezgrixp7xEfzwZHLDi5GWemxEoSiSeCnEQ4`
  - register payout address: `44e5XJFCzjjL6V3xnrVLNPfKGz3kuLk1o7kkhoFmyrBC`
  - oracle sequence `11`: `CnT6RfJjLpF5BcZR36uhxfgMDviXTpLeDKtdiUWykqrQ`
  - withdraw: `86zNEv9WJoJUY7Mn1JCfjM8buRBKJXkpJFBBRmcqpMru`
  - confirm: `CKU2wZWaDWhaHdwxNxXFm7sUhfPbEUzwCRyoKRXvtHQC`
- Remaining v1 dependency: none for guardian or funds. V1 remains only as historical on-chain state and docs/code references.

Do not run `clear_stuck_proof` on a position without checking whether a signed payout exists. Signed payouts require proof.

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
- The one-time private `migrate_legacy_ticket` path was executed and is now removed from both source and the deployed v2 wasm. The path changed no serialized fields and used a separate storage key.

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

- `cargo build --target wasm32-unknown-unknown --release` passed.
- The deployed cleanup wasm is `638233` bytes, sha256 `b03c68feda95671773ec1992579ccb3843dd18743d25bfcc76279052933e9a91`, code hash `CrxBYhxfBqcFgc7hh9dekdhNRoPLu52cQNt82QChGz7z`, deployed in tx `GJpz9wwvbJeF35qQGCgbb3ziCnxg4384HsCRLTqpYPDn`.
- The previous Phase 1 wasm was `640731` bytes, code hash `GqCZxy7v4eZp1HH8sdqpMgpbENxUVtMscabZQp4oEE6f`, deployed in tx `2r6oSSgJpxaHMkNNfcpX2iELZswh8AGub2Npy4kQh8X7`.
- Rollback wasm/state exports are saved under `/tmp/v2-*.wasm` and `/tmp/v2-state-*.json`.
- No mainnet deployment has been performed.
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

### 3a. Legacy v2 signed ticket after withdrawal confirmation (executed)

Executed on 2026-11-14 in tx `45XzfjBwYcaAWHRrqoEJ8s5yGYmrKt32SDPQPfmbM3Yz`; retained as history.

1. Confirm position `1` is `Idle`, debt is `0`, and no pending operation exists.
2. Confirm the ticket is `Signed` with liquidator `borrower.native-credit.testnet`, collateral `36_000_000_000_000_000` wei, repay `10_000_000`, and destination `0xec29df4cf529cf8be4fbc8380cf732cff82389bc`.
3. Call `migrate_legacy_ticket()` from the contract account; the method is private and one-time.
4. Confirm `legacy_ticket_migrated` and ticket status `Settled`.
5. Do not run it before the pending withdrawal is confirmed; it rejects non-idle positions and pending operations.

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

### 6. Rollback dry-run (executed, 2026-11-14)

- Rollback target for the cleanup deployment is the saved Phase 1 wasm
  `/tmp/v2-phase1-before-cleanup.wasm` (`640731` bytes, sha256
  `eb38435bc056d835543787bf9f3c89ee32e2330f2f0f97a17a40dc5130a2c124`, code hash
  `GqCZxy7v4eZp1HH8sdqpMgpbENxUVtMscabZQp4oEE6f`); it contains the already-executed one-time
  `migrate_legacy_ticket` path, but the storage guard makes it inert.
- Verified the saved wasm against the recorded previous code hash.
- Exported live state before and after the cleanup deployment:
  `/tmp/v2-state-pre-cleanup.json` and `/tmp/v2-state-post-cleanup.json` are byte-for-byte
  identical (`sha256 f31e0a31d20de53fd8cefe29d05334b0ca7e632455d3c576f552c1d3a204cf71`), so the
  cleanup upgrade changed no storage values.
- Verified the exported state includes the contract `STATE` entry, position `1`, the active deposit
  address, the legacy migration storage flag, and valid base64 records.
- Verified the cleanup wasm is deployed (`sha256
  b03c68feda95671773ec1992579ccb3843dd18743d25bfcc76279052933e9a91`, code hash
  `CrxBYhxfBqcFgc7hh9dekdhNRoPLu52cQNt82QChGz7z`) and no longer contains the string
  `migrate_legacy_ticket`.
- Live rollback, if ever needed, is a deliberate separate action: deploy the saved Phase 1 wasm,
  re-verify `get_config()`/`get_position(1)`, and keep the state export as the audit record.

## Operational risks

- Public Sepolia RPCs can rate limit, lag, or reject finalized proofs.
- The oracle remains centralized and team-operated.
- The custom consensus-critical verifier has no fuzzing or formal assurance.
- Admin actions have no timelock.
- Full liquidation leaves a small gas reserve in the position; it is below the minimum withdrawal and may remain indefinitely.
- Terminal liquidation tickets are retained in position state.
- `get_positions_by_owner` remains an unbounded scan.

## Go/no-go checklist

- [x] Changes committed and reviewed.
- [x] Legacy-ticket migration implemented and tested.
- [x] Live state exported and backed up.
- [x] Production wasm built and deployed to v2 testnet.
- [ ] Deployment/upgrade script prepared and dry-run.
- [x] Two independent Sepolia providers validated.
- [x] Live v2 withdrawal reconciled with a finality proof.
- [x] Live v1 `NeedsReview` investigated and funds recovered.
- [x] Cleanup wasm deployed; saved rollback wasm/state are available.
- [x] Rollback dry-run rehearsed and recorded; no live rollback was performed.
- [x] Watcher alerts cover refunds, signed payouts, `NeedsReview`, failed receipts, and stale prices.
- [ ] Operational alert delivery (webhook/ops channel) is configured.
