# Native Credit relayer

Generates Sepolia account proofs and broadcasts the raw Ethereum transaction
that the NEAR contract has already signed and stored.

This package is intentionally narrow:

- it never holds a NEAR or Ethereum private key;
- it only broadcasts `pending_withdrawal.raw_tx` from `get_position`;
- it checks the returned transaction hash against the contract-stored hash;
- it uses public Sepolia RPCs by default and never hard-codes a key.

## Requirements

Node 22 or newer. Install once:

```sh
cd agents/relayer
npm install --no-audit --no-fund
```

## Generate a proof

```sh
npm run proof -- \
  --address 0x0000000000000000000000000000000000000001 \
  --position 1 \
  --out /tmp/deposit-proof.json \
  --args-out /tmp/deposit-args.json
```

Defaults:

- block: `finalized`;
- two public providers: `sepolia.rpc.thirdweb.com` and
  `eth-sepolia.api.onfinality.io/public`;
- quorum: 2;
- light client: `client-eth2.sepolia.testnet`;
- finality preflight: enabled.

The command verifies the exact header RLP hash, verifies the EIP-1186 account
proof locally, compares the provider snapshots, and checks that the NEAR light
client returns the same block hash. `--out` contains metadata plus
`contract_args`; `--args-out` contains only the JSON arguments for a NEAR call.

Options:

```text
--position <id>       Add position_id to the contract args
--method <name>       record_deposit (default) or confirm_withdrawal
--block <tag>         finalized (default), latest, or a block number
--providers <urls>    Comma-separated Sepolia RPC URLs
--min-quorum <n>      Required successful providers (default: 2)
--light-client <id>   NEAR light-client account
--near-rpc <url>      NEAR RPC URL
--no-check-finality   Skip the light-client preflight
--out <path>          Full proof JSON
--args-out <path>     Contract arguments JSON only
```

Use `--min-quorum 1` only for debugging; the production path requires two
matching providers. An absent account produces a valid absence proof, but a
deposit proof must be regenerated after the account is funded.

## Submit a proof

Submit the generated arguments through `near-cli` after the contract and relayer
accounts exist:

```sh
npm run submit -- \
  --contract native-credit.testnet \
  --args /tmp/deposit-args.json \
  --signer relayer.testnet
```

The default signer uses the `near-cli` keychain; no private key is passed on the
command line or stored by this package. Preview the exact command first:

```sh
npm run submit -- \
  --contract native-credit.testnet \
  --args /tmp/deposit-args.json \
  --signer relayer.testnet \
  --dry-run
```

Options:

```text
--method <name>       record_deposit (default) or confirm_withdrawal
--network <name>      NEAR network (default: testnet)
--gas <value>         Prepaid gas (default: 150Tgas)
--deposit <value>     Attached deposit (default: 0)
--near-cli <path>     near-cli binary (default: near)
--dry-run             Print the command without sending
```

`--signer` can also come from `NEAR_SIGNER_ACCOUNT`. The contract returns the
finality callback. Do not credit the deposit from the relayer output alone.

## Broadcast a signed withdrawal or liquidation

The NEAR contract must already have a `pending_withdrawal` with `raw_tx` and
`tx_hash`. Check it first:

```sh
npm run broadcast -- \
  --contract native-credit.testnet \
  --position 1 \
  --dry-run
```

Then broadcast:

```sh
npm run broadcast -- \
  --contract native-credit.testnet \
  --position 1
```

The script reads the position through the public NEAR RPC, verifies that the
raw transaction hash matches the stored hash, and sends
`eth_sendRawTransaction` to the first working provider. It fails over to the
next provider on transport errors. It does not wait for Sepolia finality; use a
fresh proof and `confirm_withdrawal` for settlement.

The relayer never signs a withdrawal. The transaction must already have been
built and signed by the NEAR contract.

Options:

```text
--contract <id>       NEAR contract account (required)
--position <id>       Position id (required)
--providers <urls>    Comma-separated Sepolia RPC URLs
--near-rpc <url>      NEAR RPC URL
--dry-run             Print the raw transaction without broadcasting
```

## Reconcile a position

After submitting a proof or broadcasting a withdrawal, poll the contract view
until the pending operation clears:

```sh
npm run reconcile -- --contract native-credit.testnet --position 1
```

Options:

```text
--near-rpc <url>      NEAR RPC URL
--interval <ms>       Poll interval (default: 5000)
--timeout <seconds>   Timeout (default: 300)
--once                Print one snapshot and exit
--json                One-line JSON snapshots
```

The command exits `2` if the position enters `NeedsReview` and `1` on timeout.
It only reads contract state; it never signs or broadcasts.

## Environment

```sh
SEPOLIA_RPC_URLS=https://...,https://...
NEAR_RPC_URL=https://rpc.testnet.near.org
```

No API key is required with the default public endpoints. Public RPCs can rate
limit or lag; if quorum fails, retry or temporarily use one provider for
debugging.

## Tests

```sh
npm test
```

The tests use the captured Sepolia vectors under
`contracts/native-credit/testdata/` plus local mock RPC servers. They cover real
account, contract-account, absence, and tampered proofs; provider quorum and
mismatch handling; Borsh-encoded light-client calls; NEAR view decoding;
transaction failover; position reconciliation; and `near-cli` command
construction. No test sends a transaction or needs a key.
