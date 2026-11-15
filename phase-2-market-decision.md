# Phase 2 Architecture Decision

Date: 2026-11-15
Decision by: protocol owner (`uyscuttyy`)
Status: **DECIDED**

## Decision

**Option B — build the real market as a new clean contract.**

The Phase 2 marketplace is a new one-contract system that holds pool money,
lender shares, borrowing, and liquidation in a single design made for the final
product. The current contract (`v2.native-credit.testnet`) stays online as the
prototype/legacy and is not migrated.

## Design principle (explicit)

**Per-market rules are data, not code.** Every asset+chain combination is a
registered market config stored on-chain (reserve factor, collateral factor,
liquidation threshold/penalty, min amounts, caps, interest-rate curve, price
feed pair, decimals, addresses). Adding a new asset or chain is an admin
transaction, not a code change or redeploy. Only universal invariants (math,
proof verification, state machine) live in code. Per-chain proof parameters are
config; adding a new EVM chain family requires no new verifier code, only a
deployed light client for that chain.

## Why this option

- Clean data model designed for multi-asset/multi-chain from day one, so the
  Phase 3 "universal market" is an extension, not a rebuild.
- One contract = one security story; easier to audit and fuzz.
- No risky in-place storage surgery on a live deployed contract (the main
  failure mode of Option C).
- No two-contract reconciliation weak point (the main weakness of Option A).

## Consequences

- Phase 2 starts with a design/spec stage before code: storage layout, market
  registry schema, share accounting, loss waterfall, and migration path.
- The two live v2 positions remain on the legacy contract as testnet evidence;
  they are not moved.
- Multi-chain expansion later means: deploy a light client per chain, register
  new markets by admin transaction, and (only for non-EVM chains) add verifier
  code.
