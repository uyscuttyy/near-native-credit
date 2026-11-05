# HANDOFF.md

Status: **Phase 1 live: deposit, borrow, and repay completed; withdrawal broadcast and awaiting Sepolia finality.**
Last updated: 2026-11-05

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
- `web/` - Vite + React dashboard with MyNearWallet connection, live NEAR RPC
  reads, position actions, pending-operation state, and real/simulated labels.
- Empty scaffold for `agents/watcher/`, `agents/keeper/`, `agents/liquidator/`,
  `scripts/`, `docs/`.
- `native-credit.testnet` is deployed and initialized; position `1` exists,
  owned by the dedicated borrower account `borrower.native-credit.testnet`.
- No git repository, no commits, no remote, and no public push yet.

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
- views for config, position, price, and health factor.

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

1. `cargo test -p native-credit`: **31 tests pass**.
2. `cargo clippy -p native-credit --all-targets -- -D warnings`: clean.
3. `cargo build -p native-credit --target wasm32-unknown-unknown --release`:
   succeeds; wasm is `601264` bytes, sha256
   `ca03505b5e6aa23601cbaa46848eecdf74d24b2be366b0baa48312e295d6f143`,
   code hash `EbaHaLfHMuNuoSGhWBfMMJtKqBjhruryaZ31YJwVJu1C`.
4. `npm test` in `agents/relayer`: **24 tests pass**.
5. `npm test` in `agents/oracle`: **4 tests pass**; live `update_price`
   succeeded with 3-of-3 signatures, threshold 2.
6. Live proof generation with two public providers succeeded at finalized
   Sepolia block `12065248`; the header hash matched the NEAR light client via
   `block_hash_safe`.
7. Deployed `native-credit.testnet`:
   - owner/guardian `native-credit.testnet`;
   - config uses USDC `3e2210...b8af`, `client-eth2.sepolia.testnet`,
     `v1.signer-prod.testnet`, chain id `11155111`;
   - position `1` has deposit address
     `0xec29df4cf529cf8be4fbc8380cf732cff82389bc`;
   - `create_position` tx
     `BGhXuqdD6J5BrYPZQjyQvZ4QFx49VRDcuWpbS48bK63o`.
8. Live lifecycle on position `1`:
   - user funded `0.05` Sepolia ETH; proof at finalized block `12067871`
     credited `collateral_wei=50000000000000000`;
   - USDC was registered for the contract and borrower, then `borrower`
     borrowed `10 USDC`; debt reached `10000000`, health `120000` bps;
   - borrower repaid all `10 USDC` via `ft_transfer_call`, tx
     `CW8wW3p1QvakoXQDDcx8jgUyztaf4daFGFmL2b6awHLo`; debt returned to `0`;
   - `borrower` owns position `1` after `transfer_position`; withdrawal of
     `0.01` ETH was signed by MPC and broadcast as Sepolia tx
     `0x671d78716a1bcfc79931b78d2764d340cd85e866e2df902ad1ae9e8ba0b72362`
     in block `12068099`; confirmation is pending finality.
9. Real captured Sepolia vectors:
   - `testdata/sepolia_12058306.json` - 688-byte header RLP, state root,
     EOA proof for `0x...01`, USDC contract proof, derived-address absence proof.
   - `testdata/sepolia_tx_12058498.json` - real finalized Sepolia EIP-1559
     transaction; the Rust builder reproduces its exact raw bytes and hash.

## Decisions still needed

1. Confirm the repo policy: keep local until explicit push/public approval.
2. Confirm the submission payload and X demo post before publishing.

## Next steps

1. Wait for Sepolia finality, generate a `confirm_withdrawal` proof, and submit
   it to settle position `1`.
2. Add mock light-client/MPC contracts and `near-workspaces` integration tests
   for promise wiring and gas.
3. Build the watcher, keeper, and liquidator agents.
4. Build the Next.js frontend with deposit QR, pending/needs-review states,
   health factor, oracle age, risk-off, and trust labels.
5. Run mutation tests: remove each guard and show the failing test.
6. Write demo script, threat model, submission payload; ask owner to confirm
   before submitting.

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
- Public Sepolia RPCs are rate limited; `--min-quorum 1` is debugging only.
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
- Do not push, make the repo public, or submit the hackathon form without the
  owner's explicit confirmation.
