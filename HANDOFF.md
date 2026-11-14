# HANDOFF.md

Status: **Historical run log. Phase 1 v2 completed through liquidation, and the final fresh-EOA withdrawal succeeded on Sepolia and awaits NEAR confirmation. See `README.md` and `audit-report.md` for the current live state and open findings.**
Last updated: 2026-11-07

## What exists

- `RESEARCH.md` - Phase 0 findings, Mode A design, risks, and constraints.
- `HANDOFF.md` - this file.
- `VERIFY.md` - how to reproduce research and local contract checks.
- `README.md` - project overview and current status.
- Rust workspace:
  - `contracts/native-credit/` - NEAR lending contract, header parser,
    EIP-1186 account-proof verifier, EIP-1559 transaction builder.
  - `contracts/native-credit/testdata/` - captured Sepolia header/account
    proofs and a real Sepolia EIP-1559 transaction.
- `agents/relayer/` - public-RPC proof generation, light-client preflight,
  contract-argument export, raw-transaction broadcast, submit, and reconcile.
- `agents/oracle/` - 2-of-3 Ed25519 price signer and `update_price` submitter.
- `agents/shared/` - shared NEAR view helpers and pure risk/health helpers.
- `agents/watcher/` - read-only alerts for stale oracle, near/liquidatable
  health, `NeedsReview`, pending operations, Sepolia receipt status, and
  raw-tx/hash mismatches.
- `agents/keeper/` - read-only range scanner that ranks liquidation candidates.
- `agents/liquidator/` - dry-run liquidation planner; sends only with
  `--execute` and can register a Sepolia payout address first.
- `web/` - Vite + React dashboard with MyNearWallet connection, live NEAR RPC
  reads, position actions, pending-operation state, and real/simulated labels.
- `AUDIT/` - independent audit baseline and a live-evidence follow-up.
- `v2.native-credit.testnet` is deployed and initialized; position `1` exists,
  owned by `borrower.native-credit.testnet`. The retired v1 contract remains at
  `native-credit.testnet` for historical evidence.
- Git repository exists at commit `f8241dc`. No baseline tag is present. This
  run log predates the first public push; see `README.md` for current state.

## Local implementation

`contracts/native-credit/src/lib.rs` implements:

- position creation and MPC-derived Sepolia address callback;
- permissionless deposit proof submission + light-client finality callback;
- borrow, repay via USDC `ft_on_transfer`, and liquidation via USDC
  `ft_on_transfer`;
- owner-only transfer of idle, debt-free positions;
- one-in-flight operation checks per position;
- withdrawal signing through MPC, signature recovery, raw EIP-1559 assembly,
  and storage of `tx_hash`;
- withdrawal finality confirmation using a fresh account proof and nonce/balance
  delta;
- internal liquidation tickets and registered Sepolia payout addresses;
- 2-of-3-compatible Ed25519 price quorum (currently configurable, team-operated);
- owner/guardian risk-off, owner-only guardian and stuck-proof recovery;
- views for config, position, owner positions, price, and health factor.

Key guardrails implemented:

- deposits are not credited until `block_hash_safe` returns the exact header
  hash;
- withdrawals/liquidations are not settled until a later proof shows the nonce
  advanced and the balance delta is unambiguous;
- contract only signs transfers to registered Sepolia addresses, with no
  arbitrary call data and a fixed gas limit;
- EOA-only collateral accounts; contract accounts are rejected;
- outgoing transactions use low-s normalized signatures and canonical RLP;
- repayment and liquidation are not gated by risk-off; new borrowing and
  withdrawals are;
- wrong-token or malformed `ft_on_transfer` messages are refunded.

`contracts/native-credit/src/crypto.rs` implements and tests:

- generic header RLP parsing (`stateRoot` field 3, `number` field 8);
- custom Merkle-Patricia account-proof verification;
- EOA account decoding and contract-account rejection;
- absence proofs;
- MPC derived public key -> Ethereum address;
- secp256k1 recovery with low-s normalization;
- EIP-1559 signing hash and canonical raw transaction assembly;
- exact reproduction of a real Sepolia EIP-1559 transaction.

`contracts/native-credit/src/math.rs` implements checked USD conversion, health
factor, borrow limit, liquidation collateral, and simple interest accrual.

## Relayer

`agents/relayer/` is implemented and tested:

- `src/providers.mjs` - provider quorum, finalized-block resolution, snapshot
  comparison, retries;
- `src/ethereum.mjs` - exact header RLP, header hash check, EIP-1186 proof
  verification, account/absence decoding, contract-arg byte arrays;
- `src/light-client.mjs` - Borsh-encoded `block_hash_safe` preflight;
- `src/proof.mjs` - proof CLI with `--out` and `--args-out`;
- `src/near.mjs` - public NEAR RPC view calls;
- `src/submit.mjs` - builds a non-interactive `near-cli` call for
  `record_deposit`/`confirm_withdrawal` using the keychain; bare numeric
  deposits are normalized to `NEAR`;
- `src/reconcile.mjs` - polls a position until pending work clears, and flags
  `NeedsReview`;
- `src/broadcast.mjs` - reads the contract-stored raw tx, checks its hash, and
  broadcasts through a public Sepolia provider.

Defaults use the public Sepolia providers `sepolia.rpc.thirdweb.com` and
`eth-sepolia.api.onfinality.io/public`. No RPC key is required. `npm test`
covers real proof vectors, quorum mismatches, tampered proofs, light-client
encoding, NEAR view decoding, and broadcast failover.

## Oracle

`agents/oracle/` signs the exact contract message
`native-credit:v1:price:{eth_usd_1e8}:{usdc_usd_1e8}:{timestamp}:{sequence}`
with the team's 2-of-3 Ed25519 keys, fills empty slots for missing keys, checks
the on-chain threshold, and submits `update_price` through `near-cli`.

Keys live only in `~/.near-credentials/native-credit-oracle-keys.json`
(`chmod 600`), never in the repo. The contract stores the three public keys and
threshold `2`. Live prices are currently `3000e8` ETH/USD and `1e8` USDC/USD;
the sequence advances on every fresh update.

## Frontend

`web/` is a Vite + React dashboard. It reads `get_config`, `get_price`,
`get_position`, `get_health`, USDC balance, and USDC storage directly from
public NEAR RPC, then polls every 15 seconds. MyNearWallet connects through
`@near-wallet-selector` for:

- borrow and withdraw calls to the lending contract;
- USDC `ft_transfer_call` repayment;
- Sepolia payout-address registration and USDC storage registration.

The UI labels the Sepolia light-client verification as real, the team oracle as
partly simulated, and test USDC as simulated. Borrow and withdraw controls are
disabled while the oracle is stale or another proof is in flight.

Run it with:

```sh
cd web
npm install --no-audit --no-fund
npm run dev
npm run build
```

## Verified now

1. `cargo test -p native-credit`: **38 tests pass** (36 unit + 2 local E2E).
2. `cargo clippy -p native-credit --all-targets -- -D warnings`: clean.
3. `cargo build -p native-credit --target wasm32-unknown-unknown --release`:
   succeeds. `v2.native-credit.testnet` now runs the upgraded `609491`-byte wasm,
   sha256 `345e1c91175345e78fc5b49c4ea6b9b18f06ed9de7dd3fbb9447264b7f330efd`,
   code hash `4XRS7nEZJqD6JC5zbE35xMn2gG1D1PVi7yZrChHjxx3N`, deployed in tx
   `AoaLM6MDSS2nsziqgmmgwhmjpAA65KdmMGCv7gYGBoJT`. The previous deployed v2 was
   `605982` bytes, code hash `8R8LwzDfAtcLipoAiyyxfLsKhvJC8QWoGpiCpkKSBznR`; the
   retired v1 contract ran the earlier `601264`-byte wasm.
4. `npm test` in `agents/relayer`: **24 tests pass**.
5. `npm test` in `agents/oracle`: **4 tests pass**; live `update_price`
   succeeded with 3-of-3 signatures, threshold 2.
6. Live proof generation with two public providers succeeded at finalized
   Sepolia block `12065248`; the header hash matched the NEAR light client via
   `block_hash_safe`.
7. Deployed `v2.native-credit.testnet`:
   - owner `v2.native-credit.testnet`; guardian `native-credit.testnet`;
   - config uses USDC `3e2210...b8af`, `client-eth2.sepolia.testnet`,
     `v1.signer-prod.testnet`, chain id `11155111`, `gas_limit=250000` after
     the in-place upgrade;
   - USDC storage registration is funded;
   - oracle sequence `1` at `3000e8` ETH/USD and `1e8` USDC/USD;
   - position `1` has deposit address
     `0x81163cfa3c95b3691fdb4fb0766ecafcfa267999`;
   - position creation was submitted with `0.02 NEAR` storage attached;
   - the owner raised the live gas limit from `60000` to `250000` with
     `set_gas_limit` in tx `FF9QM6yfBFhHTheWvUuT5W9cJe4KStnPjvqUTHz9cRLp`;
   - live deposit proof tx
     `9MfPsqDmXfHPkNxbxNsHAkQ3b9PXRBDtW3VgbL1sJL5d` credited `0.05` ETH;
   - `borrower` borrowed `10 USDC` (tx
     `8UJixg1jFukkEPp93KzgG6Vu1vEDfE98vVV7jeNsBdjp`) and repaid it (tx
     `B5BaDvaqNths4BRD4hjnYagFseyR3jjGqT21N3U6yP2w`);
   - a `0.01` ETH withdrawal to fresh `0x0d5d...b8fa6` broadcast as
     `0x66a3a1e2e4cb7a5d70ec55ac10a0fea97255bf077035a04ef2396c70fd7fc74d`
     failed at block `12080523` with status `0x0` and `gasUsed=60000`;
     `confirm_withdrawal` settled it as failed (tx
     `EWTC7kyUXzcjC4WdjBmBNKJUXjTXYvf69ZnqBpcRbk83`);
   - a second `0.01` ETH withdrawal to the existing EOA `0xec29...89bc`
     succeeded at block `12080627` with `gasUsed=21000` (tx
     `0x0ce9ccdf7abacfd77b4d4569e46ea3dcefb512034a66d68b4e180b42d2d4ca40`)
     and settled with tx `8aSvnnnJ8NroiUUr6WXhiUgSCoGhrrK5SsUL9iHKDWHK`;
   - a controlled liquidation repaid `10 USDC` and seized `0.036` ETH (tx
     `22ckDtWYQisemyHZbhRsUj4AxX9TKfPbfaHQVXAY9p8v`; Sepolia tx
     `0x979441e8c1ea57fefa77743b223332eb9c2a3ec9895de84f648a28c9124f8109`),
     settled with tx `CZtEnZMN5GEYPvMNzqayU8bHUSVPZ4wvd8Db4BiPPMAQ`.
8. Retired v1 lifecycle on `native-credit.testnet` position `1`:
   - user funded `0.05` Sepolia ETH; proof at finalized block `12067871`
     credited `collateral_wei=50000000000000000`;
   - USDC was registered for the contract and borrower, then `borrower`
     borrowed `10 USDC`; debt reached `10000000`, health `120000` bps;
   - borrower repaid all `10 USDC` via `ft_transfer_call`, tx
     `CW8wW3p1QvakoXQDDcx8jgUyztaf4daFGFmL2b6awHLo`; debt returned to `0`;
   - `borrower` owns position `1` after `transfer_position`. The first
     `0.01` ETH withdrawal attempt was a self-transfer to the deposit address
     (`0x671d...b72362`), so it moved no net ETH.
   - A second attempt to fresh payout address `0x0d5d...b8fa6` was broadcast as
     `0x63e95e272ff6b4eb1f74c8ca814b0894a8d3f68eb7b845e546aea00319f298c2`
     and failed on Sepolia at block `12068212` with status `0x0` and
     `gasUsed=21000`; the recipient balance remained zero.
   - `confirm_withdrawal` settled the failed attempt. Position `1` is now
     `Idle` with `collateral_wei=49966999999448000`,
     `last_verified_nonce=2`, and `last_verified_block=12078967`. Only gas left
     the collateral account. See `AUDIT/FOLLOWUP.md`.
9. Agent checks:
   - `agents/watcher`: **16 tests pass**;
   - `agents/keeper`: **7 tests pass**;
   - `agents/liquidator`: **10 tests pass**.
10. Final live v2 state: position `1` is `Idle`, debt `0`,
    `collateral_wei=3897999998392000`, `last_verified_nonce=3`,
    `last_verified_block=12080729`, liquidation ticket `Settled`, and oracle
    sequence `6` at `$3000` ETH/USD. The contract holds `20 USDC`. Watcher and
    keeper report no alerts; the liquidator dry run refuses because health is
    infinite.
11. Real captured Sepolia vectors:
   - `testdata/sepolia_12058306.json` - 688-byte header RLP, state root,
     EOA proof for `0x...01`, USDC contract proof, derived-address absence proof.
   - `testdata/sepolia_tx_12058498.json` - real finalized Sepolia EIP-1559
     transaction; the Rust builder reproduces its exact raw bytes and hash.

## Decisions still needed

1. Confirm the repo policy: keep local until explicit push/public approval.
2. Confirm the submission payload and X demo post before publishing.

## Next steps

1. Top up v2 position `1` above the `0.01 ETH` minimum withdrawal, then retry
   the fresh-EOA withdrawal to `0x0d5d...b8fa6` and confirm the recipient
   balance increases. The live contract already uses `250000` gas.
2. Update the dashboard from the seeded v1 demo to the v2 position and replace
   hardcoded v1 transaction links with live pending/history data.
3. Extend the local E2E harness to a `near-workspaces` setup with mock
   light-client/MPC contracts for real promise wiring and gas. The current
   `tests/local_e2e.rs` covers the full lifecycle with mocked promise results.
4. Run mutation tests: remove each guard and show the failing test.
5. Write the demo script, threat model, and submission payload; ask the owner to
   confirm before publishing.

## Gotchas

- Do not use `eth_types::BlockHeader` from stale source to hash current Sepolia
  headers; use exact RLP bytes and generic field parsing.
- Do not call `get_trusted_signer`, `get_trusted_blocks_submitter`,
  `get_hashes_gc_threshold`, or `network` on the deployed `0.8.0` client; they
  are `MethodNotFound`.
- Do not use MPC `verify_foreign_transaction` for Sepolia; it is not available.
- Do not credit a deposit before the light client accepts the block.
- Do not complete a withdrawal before finality and a nonce advance.
- Do not use publicnode/tenderly/zan/1rpc for historical proofs; they fail.
- Public Sepolia RPCs are rate limited; proof generation requires two chain-valid providers (`--min-quorum <2` is rejected).
- Light-client view calls use Borsh arguments, not JSON numbers:
  `block_hash_safe` needs an 8-byte little-endian `u64`. A JSON number silently
  deserializes to the wrong block and returns `None`.
- Do not hard-code RPC keys or any secret.
- Init args must be wrapped as `{"config": { ... }}`; a flat config traps the
  wasm with no logs.
- `PublicKey::as_bytes()` includes the one-byte curve prefix. Oracle Ed25519
  verification must strip `[0]` and use `[1..]`, otherwise the 33-byte key
  panics inside `update_price`.
- `near-cli` rejects bare `0` deposits; use `0NEAR`.
- MPC `sign` rejects less than `15 Tgas`; the contract attaches `30 Tgas`.
- USDC `ft_transfer` rejects sender == receiver, so the position owner must be
  a different account from the lending contract.
- New USDC recipients need a `storage_deposit` before a borrow transfer.
- Sepolia now applies EIP-8037 state gas. Creating a new account costs `183600`
  state gas, so a fresh-EOA transfer estimates at ~`207391` total; v1's `21000`
  and the original v2 `60000` both fail. An existing EOA still costs `21000`.
  Live v2 was upgraded to `250000`; the fresh-address retry is still pending.
- The audit's live chain-id finding is not reproducible: `get_config` returns
  `chain_id=11155111`.
- Do not push, make the repo public, or submit the hackathon form without the
  owner's explicit confirmation.
