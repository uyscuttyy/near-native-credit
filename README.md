# Native Credit

**Status: Phase 1 live: deposit, borrow, and repay completed; withdrawal is broadcast and awaiting Sepolia finality.**

Native Credit is a testnet prototype for a lending market where Sepolia ETH can
be used as collateral and testnet USDC can be borrowed on NEAR, without wrapping
the ETH or handing custody to a bridge.

The intended flow:

1. A NEAR position owns a deterministic Sepolia deposit address derived through
   NEAR chain signatures.
2. A NEAR contract verifies the position's Sepolia ETH balance using a finalized
   Ethereum light client and an EIP-1186 account proof.
3. The position borrows testnet USDC on NEAR.
4. Repayment, liquidation, and collateral withdrawals are coordinated by the
   NEAR contract; ETH leaves the deposit address only through a transaction the
   contract constructed and a chain signature it requested.

## Current status

- Phase 0 research is documented in [`RESEARCH.md`](./RESEARCH.md).
- The NEAR contract, header/account-proof verifier, EIP-1559 builder, and
  lending/liquidation math are implemented under `contracts/native-credit/`.
- `cargo test` (31 tests), `cargo clippy`, and the wasm release build pass.
- Real Sepolia finalized-block vectors reproduce the exact header hash,
  account proof, and a real EIP-1559 transaction.
- The relayer proof generator, submit helper, signed-transaction broadcaster,
  and reconcile loop are implemented under `agents/relayer/` and tested with
  captured real Sepolia data plus local mock RPCs. It uses public Sepolia RPCs
  by default.
- The 2-of-3 Ed25519 oracle is implemented under `agents/oracle/`; live
  `update_price` calls are working.
- `native-credit.testnet` is deployed and initialized with code hash
  `EbaHaLfHMuNuoSGhWBfMMJtKqBjhruryaZ31YJwVJu1C`; position `1` is owned by
  `borrower.native-credit.testnet` and points at
  `0xec29df4cf529cf8be4fbc8380cf732cff82389bc`.
- Position `1` was funded with `0.05` Sepolia ETH, then borrowed and repaid
  `10 USDC`. A `0.01` ETH withdrawal was signed and broadcast on Sepolia; its
  confirmation is pending finality.
- The live dashboard under `web/` is built with Vite and React. It reads the
  position, oracle, risk parameters, and pending operations directly from NEAR
  RPC and connects MyNearWallet for borrow, repay, withdraw, and address
  registration.
- The watcher, keeper, and liquidator agents are not implemented yet.
- No repository has been pushed or made public.

## Key decisions

- **Mode A (recommended):** verify Sepolia collateral with the deployed
  `client-eth2.sepolia.testnet` light client and EIP-1186 account proofs.
- **Mode B (fallback):** an attested relayer/committee, clearly labeled as
  trusted. It gives up the "NEAR verifies the collateral" claim.
- The deployed `eth-prover` does not support EOA account proofs, so Mode A needs
  a small custom account-proof verifier.
- Sepolia's current execution headers include EIP-7928 and EIP-7843 fields. The
  verifier hashes the exact header RLP and parses only `state_root` and
  `number`; it does not re-serialize a typed header.
- Pyth on NEAR is stale. The prototype uses a clearly labeled 2-of-3
  team-operated Ed25519 price quorum. The keys and threshold are configured
  on-chain; the private seeds stay outside the repo.
- Deployment and initial creation are approved; liquidation tickets use an
  internal claim rather than NEP-171.

## Trust labels

Every surface must say which part is real and which part is simulated:

- `REAL TESTNET`: Sepolia, NEAR testnet, chain signatures, the light client,
  and Circle testnet USDC.
- `PARTLY SIMULATED`: the price oracle run by the team.
- `DEMO`: seeded positions, scripts, or a mock token used only if the real
  testnet token route fails.

This is not a mainnet product and must not be presented as one.

## Local verification

```sh
cargo test -p native-credit
cargo clippy -p native-credit --all-targets -- -D warnings
cargo build -p native-credit --target wasm32-unknown-unknown --release

cd agents/relayer
npm install --no-audit --no-fund
npm test
npm run proof -- --address 0x0000000000000000000000000000000000000001 --position 1

cd ../oracle
npm test
ORACLE_KEYS_FILE="$HOME/.near-credentials/native-credit-oracle-keys.json" \
  npm run update -- --contract native-credit.testnet --eth-usd 3000 --dry-run
```

See [`VERIFY.md`](./VERIFY.md) for the research checks and captured vectors.

## Run the dashboard

```sh
cd web
npm install --no-audit --no-fund
npm run dev
```

The app defaults to `native-credit.testnet` and position `1`. Override the
contract, USDC, or RPC with `VITE_CONTRACT_ID`, `VITE_USDC_ID`, and
`VITE_NEAR_RPC`. The UI labels the real, partly simulated, and demo surfaces
and refuses to enable borrow or withdraw while the oracle is stale.

## Planned layout

```
contracts/native-credit/   NEAR lending contract and proof verifier
contracts/credit-oracle/   optional signed price feed verifier
agents/relayer/            proof submission and transaction broadcasting
agents/oracle/            2-of-3 Ed25519 price signer
agents/watcher/            independent proof verification and alerts
agents/keeper/             health-factor monitor and liquidation candidates
agents/liquidator/         repayment and liquidation execution
web/                       Vite + React dashboard with MyNearWallet
scripts/                   deploy, fund, proof-generation, demo helpers
docs/                      runbooks, demo script, threat model
```

## Documentation

- [`RESEARCH.md`](./RESEARCH.md) - Phase 0 findings and the plan.
- [`HANDOFF.md`](./HANDOFF.md) - current state and next steps.
- [`VERIFY.md`](./VERIFY.md) - how to reproduce the research checks.

## Hackathon disclosure

Built for **NEAR HACKATHON** (2026-11-01 to 2026-11-14). The repository is new;
the research and all code are written in the hackathon window. Libraries and
reference projects are listed in `RESEARCH.md`. The project is testnet-only.

## License

MIT. See [`LICENSE`](./LICENSE).
