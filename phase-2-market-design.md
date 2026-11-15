# Phase 2 Market Design Spec

Date: 2026-11-15
Branch: `phase-2/market`
Decision basis: `phase-2-market-decision.md` (Option B — new clean contract)
Status: draft for review before implementation

## 1. Goal

A single new NEAR contract, `market`, that is the real product:

- Suppliers deposit assets into per-market pools and earn interest.
- Borrowers lock collateral (from a supported chain) and borrow supported
  assets.
- Everything economic is per-market data registered on-chain; code contains
  only universal invariants.

Testnet scope (first release): one chain family (EVM/Sepolia), assets = ETH
collateral + testnet USDC borrow. But the schema is written for N markets from
day one — adding a second market is an admin registration, not a code change.

## 2. Market registry (data, not code)

```text
struct MarketConfig {
  market_id: u32,
  chain_family: ChainFamily,      // Evm for now; Solana/others later
  chain_id: u64,                  // 11155111 for Sepolia
  light_client: AccountId,        // per-chain light client contract
  collateral_asset: Option<Asset>,// the collateral side, e.g. Sepolia ETH
  loan_asset: Asset,              // the borrow side, e.g. NEAR testnet USDC
  // economics (admin-settable, per market)
  collateral_factor_bps: u16,
  liquidation_threshold_bps: u16,
  liquidation_penalty_bps: u16,
  reserve_factor_bps: u16,        // cut of interest to reserves
  interest_curve: InterestCurve,  // base/slope/kink params
  min_deposit_wei: u128,
  min_borrow: u128,
  supply_cap: u128,
  borrow_cap: u128,
  oracle_pair: (AccountId, AccountId), // price feeds for both sides
}
```

Rules: a market must reference assets with verified light clients and an
oracle pair; caps and factors are checked against global safety minimums at
registration; parameter updates go through a two-step timelock (delay +
confirm), not a single call.

## 3. Storage layout (one contract, per-market state)

```text
markets: LookupMap<market_id, MarketState>
  MarketState {
    config: MarketConfig,
    // pool accounting
    total_supply: u128,            // loan-asset units supplied
    total_borrow: u128,
    total_reserves: u128,
    accrued_interest_index: u128,  // interest index per supplied unit
    last_accrual_secs: u64,
    share_price_pool: u128,        // supplier shares numerator/denominator
    // collateral lifecycle (inherits the proven v2 pattern)
    positions: LookupMap<position_id, Position>,
  }
```

Key structural choices vs v2:

- **Positions live inside their market** (a position borrows that market's
  loan asset against that market's collateral). One position, one market — no
  cross-market positions in v1 of the market contract.
- Supplier balances live in a **separate fungible-token-like ledger**
  (`supplier_shares: LookupMap<(market_id, account), u128>`) so supplier
  accounting never touches position fields (the v2 lesson).
- All interest accrual is **index-based** (interest_index moves, per-supplier
  shares are static), so accrual cost is O(1) regardless of supplier count.

## 4. Supplier flow

1. Supplier deposits loan asset → shares minted at current index.
2. Interest accrues to the pool each interaction (and via keeper for cold
   markets); suppliers' value rises with the index.
3. Withdrawal: if pool liquidity allows, instant; otherwise a **queued
   withdrawal** that clears as borrowers repay/liquidations settle. No silent
   socialization of unavailability.
4. Reserve factor: interest split (suppliers vs reserves) is per-market data.
   Reserves can be drawn by governance/timelock only, with events.

## 5. Borrower flow (inherits the proven v2 machine)

Deposit-proof → credit → borrow → repay → withdraw, using the same verified
pieces: per-position MPC-derived Sepolia address, EIP-1186 account-proof
verification against the light client, EIP-1559 MPC-signed payouts, finality
confirmation. The differences from v2 are only that position state references
its `market_id` and reads that market's config.

## 6. Loss waterfall (explicit, per market)

When a liquidation cannot recover the debt:

1. Liquidator takes available collateral (capped, penalty included).
2. Remaining bad debt is written to `total_bad_debt` for the market.
3. Bad debt is absorbed **first from reserves, then by supplier share
   dilution** — never silently; every step emits an event and the amounts are
   queryable.
4. Insurance/stability fund is a Phase 3 item (a dedicated market-like pool).

## 7. Oracle

Same Ed25519 quorum design as v2, but prices are keyed per oracle pair so
multiple markets can have independent feeds. Stale/future rules inherited.
Fallback feed + deviation circuit breaker are Phase 3 (documented as known
limitation, watchable via the existing watcher alerts).

## 8. Events (NEP-297 style from day one)

Every state transition emits a typed event: MarketRegistered,
MarketParamsUpdated, Supplied, Withdrawn, WithdrawQueued, Borrowed, Repaid,
InterestAccrued, Liquidated, BadDebtRecorded, ReservesDrawn. This gives
indexers/history without chain archaeology (the v1 lesson).

## 9. What is deliberately NOT in the first release

- Cross-market positions (one position = one market).
- Non-EVM chain families (schema supports them; verifier code does not yet).
- Stability pool / insurance fund / liquidation auctions (fixed penalty, as in
  v2, with documented rationale).
- Interest-rate fallback feed and deviation circuit breaker.
- Governance token; admin is multisig-ready but starts as the owner account
  with timelocked parameter changes.

## 10. Build order

1. `market` contract skeleton + market registry + timelocked param updates.
2. Supplier side: deposit/shares/withdraw/queue + accrual, with property and
   mutation tests (reuse the harness).
3. Borrower side: port the proven proof/signature machine onto per-market
   state.
4. Liquidation + loss waterfall with events.
5. Agents updated for multi-market scanning; dashboard reads the new contract.
6. Fresh testnet deployment; run both markets side by side before cutover.

Every step lands with tests, lint-clean code, and a commit+push.
