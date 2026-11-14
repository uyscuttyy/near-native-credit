# Native Credit: Current Architecture

**Audit snapshot:** 2026-11-14  
**Baseline commit:** `f8241dc` (working tree also contains uncommitted deployment and documentation changes)  
**Contract reviewed:** `contracts/native-credit/src/lib.rs`, `crypto.rs`, `math.rs`  
**Live network:** NEAR testnet, Ethereum Sepolia  
**Status:** Testnet prototype. The core overcollateralized credit-line path works end to end; the product is not a complete money market.

---

## 1. Scope and intent

Native Credit lets a NEAR account borrow testnet USDC against ETH that remains on Ethereum Sepolia. The contract does not wrap or bridge the ETH. Instead, it derives a per-position Sepolia address through the NEAR MPC signer, verifies Sepolia account proofs through a NEAR light client, and asks the MPC signer to sign EIP-1559 transactions that move ETH from the derived address.

The current unit of account is the **position**, not a shared pool. A position has one owner, one derived Sepolia address, one collateral balance, one USDC debt, and at most one pending cross-chain operation. There is no supplier side, no LP share, no shared liquidity pool, and no interest distribution to lenders.

The architecture is best understood as:

1. **Collateral custody on Sepolia** - ETH stays at a derived EOA.
2. **Proof verification on NEAR** - the light client proves a Sepolia header and the contract verifies an EIP-1186 account proof.
3. **Credit issuance on NEAR** - the contract transfers testnet USDC to the position owner.
4. **MPC-signed payouts on Sepolia** - withdrawals and liquidation payouts are signed by the MPC and broadcast by an off-chain relayer.

---

## 2. System at a glance

```text
                         ┌──────────────────────────────┐
                         │        NEAR testnet          │
                         │                              │
  borrower/liquidator ──▶│  native-credit contract      │
        (NEAR)           │  - positions                 │
                         │  - USDC debt                 │
                         │  - oracle price              │
                         │  - proof verification        │
                         └───────┬───────────┬──────────┘
                                 │           │
                    block_hash_safe│         │sign / derived_public_key
                                 ▼           ▼
                         ┌──────────────┐  ┌────────────────────┐
                         │ light client │  │ MPC signer         │
                         │ (Sepolia)    │  │ v1.signer-prod     │
                         └──────┬───────┘  └─────────┬──────────┘
                                │                    │
                                ▼                    ▼
                         ┌────────────────────────────────────┐
                         │        Ethereum Sepolia            │
                         │  derived EOA per position          │
                         │  ETH collateral                    │
                         └──────────────▲─────────────────────┘
                                        │
                              relayer broadcasts
                              signed raw transactions

  oracle signers ──▶ update_price(price, signatures) ──▶ contract
  agents/watcher, keeper, liquidator ──▶ monitoring and liquidation planning
  web/ ──▶ demo management UI for position 1
```

---

## 3. Components

| Component | Path | Responsibility | Current state |
|---|---|---|---|
| Credit contract | `contracts/native-credit/src/lib.rs` | Positions, deposits, borrow, repay, withdraw, liquidation, oracle, proof callbacks | Implemented; core path live-tested |
| Crypto verifier | `contracts/native-credit/src/crypto.rs` | RLP, Keccak, MPT account proofs, EOA checks, EIP-1559 assembly, secp256k1 recovery | Implemented; custom and under-assured |
| Math | `contracts/native-credit/src/math.rs` | USD conversion, health factor, borrow limit, interest, liquidation seize | Implemented; unit-tested |
| Local E2E | `contracts/native-credit/tests/local_e2e.rs` | Full local deposit/borrow/repay/withdraw/liquidation harness | Implemented; 2 tests |
| Relayer | `agents/relayer/` | Generate proofs, submit NEAR calls, broadcast signed Sepolia tx, reconcile | Implemented; live path used single-provider quorum |
| Oracle | `agents/oracle/` | Sign and submit Ed25519 price updates | Implemented; team-operated |
| Watcher | `agents/watcher/` | Monitor positions, pending operations, health, receipt status | Implemented; unit-tested |
| Keeper | `agents/keeper/` | Scan positions, identify liquidatable accounts | Implemented; unit-tested |
| Liquidator | `agents/liquidator/` | Plan and execute liquidation, including Sepolia payout | Implemented; unit-tested |
| Shared agent lib | `agents/shared/` | NEAR RPC, borsh helpers, risk math, canonical JSON | Implemented |
| Web demo | `web/` | Wallet connection and position-1 management | Partial demo |
| Documentation | `README.md`, `HANDOFF.md`, `VERIFY.md`, `AUDIT/` | Runbooks and prior evidence | Partially stale |

---

## 4. Contract data model

The contract is defined in `contracts/native-credit/src/lib.rs`.

### 4.1 `Config` (L33-52)

| Field | Meaning |
|---|---|
| `owner` | Admin account; set to the deployer/predecessor in `new` (L174-224) |
| `guardian` | Can toggle `risk_off`; set at initialization |
| `usdc` | NEP-141 USDC contract account |
| `light_client` | Sepolia light-client contract |
| `mpc` | MPC signer contract, e.g. `v1.signer-prod.testnet` |
| `oracle_keys` | Ed25519 public keys |
| `oracle_threshold` | Minimum valid signatures |
| `collateral_factor_bps` | Borrow limit factor |
| `liquidation_threshold_bps` | Health-factor threshold |
| `liquidation_penalty_bps` | Liquidator collateral bonus |
| `interest_rate_bps` | Simple annual borrower interest |
| `max_oracle_age_secs` | Maximum price age |
| `min_withdrawal_wei` | Minimum Sepolia ETH withdrawal |
| `min_borrow_usdc` | Minimum USDC borrow |
| `risk_off` | Blocks new borrow/withdraw when true |
| `chain_id` | Sepolia chain id, expected `11155111` |
| `gas_limit` | Sepolia transaction gas limit |
| `max_fee_per_gas` | EIP-1559 max fee |
| `max_priority_fee_per_gas` | EIP-1559 priority fee |

`new` validates collateral ordering, liquidation penalty, oracle threshold, Ed25519 key type, gas configuration, chain id, and minimum amounts. It overwrites `config.owner` with `env::predecessor_account_id()`.

### 4.2 `Price` (L65-70)

```rust
eth_usd_1e8: U128
usdc_usd_1e8: U128
timestamp: u64
sequence: u64
```

Price updates require a strictly increasing sequence, positive values, a timestamp no more than 60 seconds in the future, freshness within `max_oracle_age_secs`, and at least `oracle_threshold` valid Ed25519 signatures over the canonical message (L788-833). The signed message is:

```text
native-credit:v1:price:{eth_usd_1e8}:{usdc_usd_1e8}:{timestamp}:{sequence}
```

### 4.3 `Position` (L143-156)

| Field | Meaning |
|---|---|
| `owner` | NEAR account that owns the position |
| `status` | `Deriving`, `Idle`, `WithdrawalPending`, `LiquidationPending`, `NeedsReview`, or `Closed` |
| `deposit_address` | Derived Sepolia EOA, set after MPC callback |
| `collateral_wei` | Last verified Sepolia balance credited as collateral |
| `last_verified_block` | Sepolia block number of the last accepted account proof |
| `last_verified_nonce` | Sepolia account nonce of the last accepted proof |
| `debt_usdc` | Outstanding USDC debt, including accrued interest |
| `last_accrual_secs` | Timestamp of the last interest accrual |
| `pending_deposit` | One pending deposit or withdrawal proof |
| `pending_withdrawal` | One pending signed or unsigned Sepolia payout |
| `pending_borrow` | One in-flight USDC transfer |
| `liquidation_ticket` | Optional liquidation payout record |

There is no stored `derivation_path` and no per-position asset list.

### 4.4 Pending types

`PendingProof` (L92-101):

```rust
header_hash: [u8; 32]
header_number: u64
new_balance: U128
new_nonce: u64
kind: Deposit | Withdrawal
requested_block: u64
```

`PendingWithdrawal` (L110-122):

```rust
amount_wei: U128
max_gas_wei: U128
nonce: u64
to: String
signing_hash: [u8; 32]
raw_tx: Option<Vec<u8>>
tx_hash: Option<[u8; 32]>
kind: User | Liquidation
requested_block: u64
```

`LiquidationTicket` (L133-139):

```rust
liquidator: AccountId
collateral_wei: U128
repay_usdc: U128
sepolia_address: String
status: PendingSignature | Signed | Settled | Cancelled
```

### 4.5 Contract-level accounting

`Contract` (L160-169) stores:

| Field | Current use |
|---|---|
| `config` | Global configuration |
| `positions` | `LookupMap<u64, Position>` |
| `sepolia_addresses` | `LookupMap<AccountId, String>` for payout recipients |
| `next_position_id` | Starts at 1 |
| `total_debt_usdc` | Incremented/decremented on borrow, repay, liquidation, accrual |
| `total_collateral_wei` | Incremented/decremented on verified deposit/withdrawal |
| `total_bad_debt_usdc` | Initialized to zero and never updated |
| `price` | Latest accepted oracle price |

---

## 5. Position state machine

```text
create_position
      │
      ▼
   Deriving ──on_derived_public_key──▶ Idle
                                        │
             register_sepolia_address ──┘
                                        │
          record_deposit / on_block_hash
                                        │
                                        ▼
                                       Idle
                                        │
                    ┌───────────────────┼───────────────────┐
                    │                   │                   │
                 borrow              withdraw           liquidate
                    │                   │                   │
                    ▼                   ▼                   ▼
              pending_borrow     WithdrawalPending   LiquidationPending
                    │                   │                   │
             on_borrow_transfer   on_sign/confirm      on_sign/confirm
                    │                   │                   │
                    ▼                   ▼                   ▼
                   Idle                Idle                Idle / Closed
                                        │
                              proof mismatch / abnormal delta
                                        ▼
                                   NeedsReview
```

Important transitions:

- `create_position` starts at `Deriving` and requests `derived_public_key` (L226-250, L938-953).
- `on_derived_public_key` stores the derived address and moves to `Idle` (L282-301); failure leaves the position in `Deriving`.
- `record_deposit` requires `Idle`, parses and checks the Sepolia account proof, stores `pending_deposit`, then requests `block_hash_safe` (L308-343).
- `borrow` requires `Idle`, no risk-off, no pending operation, minimum amount, fresh price, and health/limit compliance; it stores `pending_borrow` and calls USDC `ft_transfer` (L344-392).
- `withdraw` requires `Idle`, registered Sepolia payout address, minimum amount, no pending operation, and post-withdrawal health/limit compliance; it stores a user withdrawal and requests an MPC signature (L413-485).
- `confirm_withdrawal` accepts only `WithdrawalPending` or `LiquidationPending`, requires a signed raw transaction, verifies a fresh account proof and a higher nonce, then requests `block_hash_safe` (L486-532).
- `on_sign` reconstructs the signed transaction, verifies that the recovered signer equals the derived address, stores `raw_tx` and `tx_hash`, and currently marks any liquidation ticket `Signed` (L533-590).
- `on_block_hash` checks the light-client header hash and finalizes deposit or withdrawal accounting (L591-683).
- `ft_on_transfer` handles `"repay"` and `"liquidate"` actions from the USDC contract (L684-787).
- `clear_stuck_proof` is owner-only, requires `NeedsReview`, waits for a 100,000-block timeout when a deposit proof exists, then clears pending deposit/withdrawal and sets `Idle` (L864-887).

The status name `Closed` is used in the withdrawal success path when debt is zero and collateral is zero (L663-668). There is no explicit close function and no storage cleanup.

---

## 6. End-to-end flows

### 6.1 Create position and derive address

1. User calls `create_position` with at least `STORAGE_DEPOSIT_YOCTO`.
2. Contract allocates `next_position_id`, stores the position in `Deriving`, and calls the MPC:
   - path: `native-credit:v1:{chain_id}:position:{position_id}`
   - method: `derived_public_key`
3. MPC callback `on_derived_public_key` converts the public key to an Ethereum address and stores `deposit_address`.
4. User may call `register_sepolia_address` to register a different payout address for that NEAR account. This is required before user withdrawal and before the account can liquidate a position.

The derivation path is deterministic per chain id and position id (`derivation_path`, L1037-1042). The live v2 position uses path `native-credit:v1:11155111:position:1`.

### 6.2 Deposit

1. User sends ETH on Sepolia to `deposit_address`.
2. User/agent supplies a Sepolia header RLP and EIP-1186 account proof to `record_deposit`.
3. Contract verifies:
   - header parses and block number is newer than `last_verified_block`;
   - account proof root matches the header state root;
   - the account is a plain EOA;
   - nonce is not older than `last_verified_nonce`;
   - balance is not below recorded `collateral_wei`.
4. Contract stores `pending_deposit` and calls the light client `block_hash_safe(block_number)`.
5. `on_block_hash` compares the returned hash with `keccak256(header_rlp)`.
6. On match, the deposit branch credits the new balance, updates block/nonce, and adjusts `total_collateral_wei`.

No deposit is credited before the light-client callback. This is the key safety property of the deposit path.

### 6.3 Borrow

1. Owner calls `borrow(position_id, amount)`.
2. Contract accrues interest, reads a fresh price, computes collateral USD, debt USD, and borrow limit.
3. It requires `new_debt_usd <= collateral_usd * collateral_factor_bps / 10_000`.
4. It stores `pending_borrow` and calls USDC `ft_transfer` to the owner.
5. Callback `on_borrow_transfer` adds the amount to debt and `total_debt_usdc` on success, or clears `pending_borrow` on failure.

USDC is transferred directly from the credit contract. The contract must already hold USDC. There is no pool accounting and no liquidity check beyond the token transfer succeeding.

### 6.4 Repay

Repayment is not a standalone method. The owner calls USDC `ft_transfer_call` with:

```json
{"action": "repay", "position_id": 1}
```

`ft_on_transfer` accepts only the position owner, accrues interest, accepts `min(amount, debt)`, reduces debt and `total_debt_usdc`, and returns any excess USDC to the sender. There is no collateral release on repay; collateral remains until withdrawal.

### 6.5 Withdraw

1. Owner registers a Sepolia payout address with `register_sepolia_address`.
2. Owner calls `withdraw(position_id, amount)`.
3. Contract requires:
   - `Idle`, no risk-off, no pending operation;
   - amount at least `min_withdrawal_wei` and greater than `max_gas_wei`;
   - remaining collateral after `amount + max_gas_wei` passes both health factor and borrow-limit checks.
4. Contract constructs an EIP-1559 transaction with:
   - nonce = `last_verified_nonce`;
   - `to` = registered Sepolia address;
   - `value` = amount;
   - configured gas limit and fees.
5. Contract stores a `User` pending withdrawal and requests an MPC signature.
6. `on_sign` reconstructs the raw transaction from the MPC signature and stores it.
7. Relayer broadcasts the raw transaction on Sepolia.
8. Owner/agent calls `confirm_withdrawal` with a fresh header and account proof showing a higher nonce and lower balance.
9. `on_block_hash` classifies the balance delta:
   - between `amount` and `amount + max_gas_wei`: success;
   - below `amount`: failed;
   - above `amount + max_gas_wei`: `NeedsReview`.

A successful user withdrawal moves the position to `Idle` (or `Closed` if debt and collateral are both zero). A failed user withdrawal also moves to `Idle`; the withdrawal is not retried automatically.

### 6.6 Liquidation

1. Liquidator must be registered in `sepolia_addresses`; that address receives seized ETH.
2. Liquidator calls USDC `ft_transfer_call` with:

```json
{"action": "liquidate", "position_id": 1}
```

3. `ft_on_transfer` requires the position to be `Idle` with no pending operation, accrues interest, reads a fresh price, and computes health.
4. Liquidation is allowed only when `health < 10_000` bps.
5. Contract computes `seized = min(collateral, repay_usd * (1 + penalty) / ETH price)`.
6. It reduces debt and `total_debt_usdc`, creates a `PendingSignature` liquidation ticket, and requests an MPC-signed Sepolia payout to the liquidator's registered address.
7. `on_sign` marks the ticket `Signed`.
8. Relayer broadcasts the payout; `confirm_withdrawal` verifies the Sepolia proof.
9. `on_block_hash` marks the ticket `Settled` on a valid payout, `Cancelled` on a failed payout, or `NeedsReview` on an abnormal delta.

The contract supports partial liquidation because `accepted = min(amount, debt)`. There is no close factor beyond the debt cap and no auction.

### 6.7 Oracle update

1. Off-chain oracle signers sign the canonical price message with Ed25519.
2. Anyone submits `update_price(price, signatures)`.
3. Contract checks sequence, positive values, timestamp freshness, and counts valid signatures in the configured key order.
4. If `valid >= oracle_threshold`, the price is stored.

The oracle is a team-operated threshold feed. It is not an on-chain DEX TWAP and has no independent fallback feed.

---

## 7. Proof and finality model

The contract does not trust a relayer assertion. It verifies:

1. **Header inclusion** - the supplied RLP header must hash to a header that the light client reports as safe.
2. **Account inclusion** - the account proof must lead from the header state root to the derived address, using the custom MPT verifier.
3. **EOA shape** - the account must have no code and no storage root indicating a contract.
4. **Monotonicity** - block number and nonce must advance; balance cannot decrease on a deposit.
5. **Withdrawal classification** - the observed balance delta determines success/failure/review.

The light client call is `block_hash_safe(block_number)`; the contract does not call `block_hash` for unsafe blocks. The relayer must choose a Sepolia block that is both finalized enough for the RPC provider and accepted by the light client.

The verifier verifies **account proofs only**. It does not prove arbitrary storage slots. That is sufficient for balances and nonces today, but it limits future contract-based collateral or more complex state.

---

## 8. Agent architecture

### 8.1 Relayer (`agents/relayer/`)

Responsibilities:

- derive Sepolia addresses and check balances;
- build EIP-1186 account proofs;
- resolve a finalized Sepolia block;
- submit `record_deposit` / `confirm_withdrawal`;
- broadcast signed raw transactions;
- reconcile pending operations and flag `NeedsReview`.

Provider behavior is important:

- `agents/relayer/src/providers.mjs` lists `sepolia.rpc.thirdweb.com` and `eth-sepolia.api.onfinality.io/public`.
- The intended production trust model is a multi-provider proof quorum, but live operations used `--min-quorum 1` because public providers disagreed or rejected pinned-finalized `eth_getProof` requests.
- `VERIFY.md` L93-114 documents this workaround and states that single-provider quorum is not the production model.

### 8.2 Oracle (`agents/oracle/`)

- Builds and signs the canonical price message with Ed25519 keys.
- Submits `update_price`.
- Operates as a 2-of-3 team feed in the live deployment.
- Has no on-chain slashing, key rotation, or independent price source.

### 8.3 Watcher, keeper, liquidator

- `watcher` monitors position state, health, pending operations, Sepolia receipt status, and `NeedsReview`.
- `keeper` scans positions and identifies liquidatable accounts.
- `liquidator` plans and executes liquidation, including the USDC repayment and Sepolia payout.
- All have unit tests; there is no live multi-agent orchestration test in this repository.

### 8.4 Shared library

`agents/shared/` contains NEAR RPC helpers, borsh encoding, canonical JSON, and risk math shared by agents.

---

## 9. Frontend

`web/` is a Vite/React demo, not a complete product UI.

Current behavior:

- connects to NEAR testnet via MyNearWallet;
- defaults to the v2 contract but still contains stale v1 copy and a hardcoded v1 withdraw transaction link;
- manages hardcoded position `1`;
- supports borrow, repay, withdraw request, payout-address registration, and USDC storage registration;
- does not create positions, discover positions by owner, verify deposits, broadcast or confirm withdrawals, liquidate, or obtain test USDC.

The frontend is useful for demoing one known position. It is not a self-serve interface for the contract's full lifecycle.

---

## 10. Live deployment snapshot

### 10.1 v2 contract

| Item | Value |
|---|---|
| Account | `v2.native-credit.testnet` |
| Wasm size | `609491` bytes |
| Wasm sha256 | `345e1c91175345e78fc5b49c4ea6b9b18f06ed9de7dd3fbb9447264b7f330efd` |
| Code hash | `4XRS7nEZJqD6JC5zbE35xMn2gG1D1PVi7yZrChHjxx3N` |
| Deploy tx | `AoaLM6MDSS2nsziqgmmgwhmjpAA65KdmMGCv7gYGBoJT` |
| Owner | `v2.native-credit.testnet` |
| Guardian | `native-credit.testnet` |
| USDC | `3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af` |
| Light client | `client-eth2.sepolia.testnet` |
| MPC | `v1.signer-prod.testnet` |
| Chain id | `11155111` |
| Gas limit | `250000` |
| Max fee per gas | `2000000000` |
| Max priority fee | `1000000000` |
| Oracle keys | 3 Ed25519 keys, threshold `2` |
| USDC balance | `20000000` (20 USDC) |
| Price | ETH `$3000`, USDC `$1`, sequence `7`, timestamp `1794021815` |

`set_gas_limit` tx: `FF9QM6yfBFhHTheWvUuT5W9cJe4KStnPjvqUTHz9cRLp`.

### 10.2 v2 position 1

| Item | Value |
|---|---|
| Owner | `borrower.native-credit.testnet` |
| Deposit address | `0x81163cfa3c95b3691fdb4fb0766ecafcfa267999` |
| Status | `WithdrawalPending` |
| Collateral | `13897999998392000` wei (`0.013897999998392 ETH`) |
| Debt | `0` USDC |
| Last verified block | `12080892` |
| Last verified nonce | `3` |
| Oracle sequence | `7` |
| Pending withdrawal | to `0x0d5d9acdb6adf6dce00d14972f7ce7b1e99b8fa6`, nonce `3`, amount `0.01 ETH`, `max_gas_wei=500000000000000` |
| Sepolia tx | `0x2a90b77ad94583af484e0f25fdf3f33a69c4d9e60869fbf7830b86738271f58f` |
| Receipt | block `12080980`, status `0x1`, `gasUsed=204600` |
| NEAR confirmation | Not submitted at audit time |
| Liquidation ticket | `Signed` (liquidator `borrower.native-credit.testnet`, collateral `0.036 ETH`, repay `10 USDC`, address `0xec29...89bc`) |

The Sepolia payout succeeded, but the NEAR position is still `WithdrawalPending` because `confirm_withdrawal` has not been called. This is expected until proof submission. The `Signed` ticket is not the expected terminal state and is discussed in the security review.

### 10.3 v1 contract and position

| Item | Value |
|---|---|
| Account | `native-credit.testnet` |
| Gas limit | `21000` |
| Deposit address | `0xec29df4cf529cf8be4fbc8380cf732cff82389bc` |
| Status | `NeedsReview` |
| Collateral | `49966999999448000` wei (`0.049966999999448 ETH`) |
| Debt | `0` USDC |
| Last verified block | `12080729` |
| Last verified nonce | `1` |
| Oracle sequence | `10` |
| Pending withdrawal | to `0x81163cfa3c95b3691fdb4fb0766ecafcfa267999`, nonce `2`, amount `0.01 ETH` |
| Sepolia tx | `0xf5dc0ee8f56a6899c28d752fc9c64726e448aceb2063128db78e865698ff5fb0` |
| NEAR confirm tx | `7wrKoqyPqq5GGJbyQykknz7Nu9Tf9YhJaYMGrQtcTuZf` |

The v1 position entered `NeedsReview` after a liquidation payout increased its Sepolia balance above the recorded collateral. It is not automatically reconciled.

---

## 11. Trust boundaries and centralization

| Boundary | Trust assumption |
|---|---|
| MPC signer | Derived address controls collateral; MPC must not sign unauthorized transactions |
| Light client | Must report the correct safe Sepolia header hash |
| Sepolia RPCs | Relayer proof source; public providers can disagree or be unavailable |
| Oracle | 2-of-3 team keys; threshold signature sets the price |
| Contract owner | Sets guardian, gas limit, and receives admin controls; contract is upgradeable at the NEAR account level |
| Guardian | Can set `risk_off`, blocking new borrow/withdraw |
| Relayer | Chooses when to submit proofs and broadcast transactions, but cannot forge proofs |
| USDC issuer | Testnet token; its behavior is external |
| Frontend | Untrusted convenience layer; reads contract state and submits wallet transactions |

There is no timelock, no on-chain governance, no event log, and no documented key-management or incident-response procedure in the repository.

---

## 12. Tests and operational evidence

Verified test counts at audit time:

| Suite | Result |
|---|---|
| Contract unit tests | `36` passed |
| Contract local E2E | `2` passed |
| Relayer | `24` passed |
| Watcher | `16` passed |
| Keeper | `7` passed |
| Liquidator | `10` passed |
| Oracle | `4` passed |
| Web | No test suite |

Contract checks previously verified: `cargo fmt --check`, clippy, and wasm release build. These passing checks do not constitute a security audit.

Important evidence from the live run:

- deposit proof tx: `9MfPsqDmXfHPkNxbxNsHAkQ3b9PXRBDtW3VgbL1sJL5d`;
- borrow tx: `8UJixg1jFukkEPp93KzgG6Vu1vEDfE98vVV7jeNsBdjp`;
- repay tx: `B5BaDvaqNths4BRD4hjnYagFseyR3jjGqT21N3U6yP2w`;
- failed fresh-address withdrawal: Sepolia `0x66a3a1e2e4cb7a5d70ec55ac10a0fea97255bf077035a04ef2396c70fd7fc74d`, block `12080523`, `status=0x0`, `gasUsed=60000`;
- existing-address withdrawal: Sepolia `0x0ce9ccdf7abacfd77b4d4569e46ea3dcefb512034a66d68b4e180b42d2d4ca40`, block `12080627`, `gasUsed=21000`;
- liquidation payout: Sepolia `0x979441e8c1ea57fefa77743b223332eb9c2a3ec9895de84f648a28c9124f8109`, block `12080719`, `gasUsed=21000`;
- liquidation NEAR call: `22ckDtWYQisemyHZbhRsUj4AxX9TKfPbfaHQVXAY9p8v`;
- liquidation confirmation: `CZtEnZMN5GEYPvMNzqayU8bHUSVPZ4wvd8Db4BiPPMAQ`.

---

## 13. Current limitations visible in the architecture

1. **No supplier market.** The contract issues USDC but has no lender deposits, LP shares, utilization curve, reserves, or interest distribution.
2. **One collateral asset and one debt asset.** ETH only, USDC only, Sepolia only.
3. **One pending operation per position.** This is simple but creates liveness and recovery problems.
4. **Manual proof submission.** The contract depends on agents and public RPCs for every deposit and withdrawal finalization.
5. **No event log.** State changes are observable only by querying positions; there is no NEP-297 event stream.
6. **No storage lifecycle.** Closed positions remain in `LookupMap`; `get_positions_by_owner` scans all position ids.
7. **No bad-debt accounting.** `total_bad_debt_usdc` is never written.
8. **Admin actions lack timelocks and events.** Owner/guardian changes are immediate.
9. **The demo UI does not exercise the protocol lifecycle.** It is a position-1 management screen.

The architecture is a coherent prototype for MPC-backed, proof-verified cross-chain credit lines. It is not yet the architecture of a universal money market.
