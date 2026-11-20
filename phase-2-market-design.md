# Phase 2 Market Design Spec

Date: 2026-11-15 (updated same day: cross-asset model decision)
Branch: `phase-2/market`
Decision basis: `phase-2-market-decision.md` (Option B — new clean contract)
Status: cross-asset model approved by owner; draft for implementation

## 1. Goal

A single new NEAR contract, `market`, that is the real product:

- Suppliers deposit any supported asset into that asset's pool and earn interest.
- Borrowers lock any enabled collateral asset and borrow any enabled loan asset.
- Positions hold a basket: multiple collateral deposits + multiple borrows.
- Everything economic is per-asset data registered on-chain; code contains only
  universal invariants.

Testnet scope (first release): EVM family (Sepolia), assets = native ETH
(collateral-only) + NEP-141 testnet tokens (e.g. USDC, USDT). The schema is
written for N assets across M chains from day one — enabling a new asset or
chain is an admin registration, not a code change.

## 2. Asset registry (data, not code)

Each supported asset is one registry entry with its own config:

```text
struct AssetConfig {
  asset: Asset,                    // kind (native eth / NEP-141), account, decimals
  family: ChainFamily,             // Evm for now; others later
  chain_id: u64,                   // 11155111 for Sepolia
  light_client: AccountId,         // per-chain proof source
  supply_enabled: bool,            // can be deposited and borrowed
  collateral_enabled: bool,        // can count as collateral
  // economics (admin-settable per asset, timelocked)
  collateral_factor_bps: u16,
  liquidation_threshold_bps: u16,
  liquidation_penalty_bps: u16,
  reserve_factor_bps: u16,
  interest_curve: InterestCurve,   // base/slope/kink
  min_deposit: u128,
  min_borrow: u128,
  supply_cap: u128,
  borrow_cap: u128,
  price_feed: AccountId,           // per-asset price feed
}
```

Rules: native ETH is collateral-only in this release (it cannot be a borrow
asset); NEP-141 tokens can be supply-enabled, collateral-enabled, or both;
every asset must enable at least one role; uniqueness is (chain, asset
account); economics are validated at registration AND at every timelocked
update.

## 3. Storage layout

```text
assets: LookupMap<asset_id, AssetEntry>     // AssetEntry { config, pool }
asset_keys: LookupSet<(chain_id, account)>  // uniqueness
pending: LookupMap<asset_id, PendingParamChange>

AssetPoolState {
  total_supply: u128,            // supplied units of this asset
  total_borrow: u128,            // borrowed units of this asset
  total_reserves: u128,
  interest_index: u128,          // starts at 10^18
  last_accrual_secs: u64,
}

supplier_shares: LookupMap<(asset_id, account), u128>   // supplier ledger
positions: LookupMap<position_id, Position>

Position {
  owner: AccountId,
  collateral: Map<asset_id, u128>,   // locked amounts per asset
  borrows: Map<asset_id, u128>,      // debts per asset
  // per-position chain proof state (inherits proven v2 pieces)
  deposit_address: Option<String>,
  last_verified_block: u64,
  last_verified_nonce: u64,
  pending_deposit / pending_withdrawal / pending_borrow ...
  liquidation_ticket: Option<LiquidationTicket>,
}
```

Key structural choices (v2 lessons applied):

- Supplier accounting lives in its own ledger, never inside position fields.
- Interest accrual is index-based: O(1) per accrual regardless of supplier count.
- Health is computed across the position's whole basket: sum of (collateral
  value x collateral factor) vs total borrowed value, using each asset's price
  feed and config.

## 4. Supplier flow

1. Deposit asset X into pool X → shares minted at the current index.
2. Interest accrues to the pool per interaction (and via keeper for cold
   pools); supplier value rises with the index.
3. Withdraw: instant if pool liquidity allows; otherwise a queued withdrawal
   that clears as borrowers repay/liquidate. No silent socialization.
4. Reserve factor: interest split (suppliers vs reserves) is per-asset data;
   reserves can only be drawn through governance/timelock with events.

## 5. Borrower flow

A position locks collateral from any enabled collateral asset (chain proofs
come from that asset's light client) and borrows any supply-enabled asset.
The proven v2 machine is reused per chain family: MPC-derived addresses,
EIP-1186 account-proof verification, EIP-1559 MPC-signed payouts, finality
confirmation. First release: one chain family (EVM), so one proof machine.

## 6. Loss waterfall (explicit, per asset pool)

1. Liquidation takes available collateral (capped, penalty included).
2. Remaining bad debt on asset X is written to X's `total_bad_debt`.
3. Bad debt is absorbed first from X's reserves, then by supplier share
   dilution in X — never silently; every step emits an event and is queryable.
4. Insurance/stability fund is a Phase 3 item.

## 7. Oracle

Same Ed25519 quorum design as v2, keyed per price feed so each asset has an
independent feed. Stale/future rules inherited. Fallback feed + deviation
circuit breaker are Phase 3 (documented, watchable via existing alerts).

## 8. Events (NEP-297 style from day one)

AssetRegistered, AssetParamsUpdated, Supplied, Withdrawn, WithdrawQueued,
Borrowed, Repaid, InterestAccrued, Liquidated, BadDebtRecorded,
ReservesDrawn, plus per-asset registration/updates. Indexers reconstruct
history without chain archaeology.

## 9. What is deliberately NOT in the first release

- Non-EVM chain families (schema ready; verifier code does not exist yet).
- Stability pool / insurance fund / liquidation auctions (fixed penalty, as in
  v2, with documented rationale).
- Oracle fallback feed and deviation circuit breaker.
- Governance token; admin is the owner account with timelocked parameter
  changes.

## 10. Build order

1. Asset registry: `register_asset`, validation, uniqueness, per-asset pool
   state, timelocked parameter updates (owner + tests). Done
   (`5991342`, cross-asset model).
2. Supplier side: deposit/shares/withdraw/queue + index accrual, with property
   and mutation tests (reuse the harness). Done (`1c96b1e`, 39 tests).
3. Borrower side: multi-asset positions (basket health, borrow/repay per
   asset), porting the proven proof/signature machine. Done
   (`fc06464`+`a62a2cc`, 80 tests, 20 mutants killed). ← current step: 4
4. Liquidation + loss waterfall with events.
5. Agents updated for multi-asset scanning; dashboard reads the new contract.
6. Fresh testnet deployment; run alongside v2 before cutover.

Every step lands with tests, lint-clean code, and a commit+push.
