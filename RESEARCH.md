# RESEARCH.md - Phase 0 findings

Status: **Phase 0 research complete. No product code has been written.**
Date: 2026-11-03
Project: **Native Credit** (working name)
Repository: `/home/uyscutty/projects/near`, local only, not a git repository yet.

> **Gate:** do not start implementation until the owner has read this document
> and approved the Mode A plan, the testnet account/deploy step, and the oracle
> approach. Nothing here authorizes a push, a public repository, a mainnet
> action, or spending.

How to read this document:

- **VERIFIED** means I read the current official documentation, read published
  source, or ran a command against a live test network and saw the result.
- **TO PROVE** means it is the plan but the experiment has not been run yet.
  Phase 1 exists to turn these into verified facts.
- **OPEN** means a decision or risk that needs the owner or more research.

The research scripts and temporary dependencies live in `/tmp/opencode`. They
are not part of the deliverable and contain no secrets.

---

## 1. Recommendation

Build **Mode A**: a NEAR contract that verifies Sepolia collateral with the
already-deployed Ethereum light client and EIP-1186 account proofs, and uses
NEAR chain signatures to move that collateral out.

Why:

- The Sepolia light client is live and its finalized block hash matches Sepolia
  exactly (VERIFIED, section 5.1).
- An EIP-1186 account proof for a plain EOA verifies against that finalized
  state root (VERIFIED in JavaScript, section 5.3).
- The current Sepolia header format can be serialized and hashed so that the
  hash matches the light client's `block_hash_safe` value (VERIFIED, section 5.2).
- The existing `eth-prover` contract does **not** verify EOA account balances;
  a small custom account-proof verifier is required (VERIFIED, section 5.4).
- Chain signatures can derive a deterministic Sepolia address and sign an
  EIP-1559 transaction; the testnet MPC does not expose `verify_foreign_transaction`
  for Ethereum, so we must not plan on that API (VERIFIED, section 4.4).

**Mode B** (an attested relayer or committee that reports Sepolia balances, with
the light client as an optional audit trail) is the fallback. It is simpler but
it gives up the "NEAR verifies the collateral" claim. If Mode B is used, the UI
and README must say so plainly. I recommend spending the extra day on Mode A.

---

## 2. Hackathon constraints (VERIFIED)

Source: <https://nearhackathon.com/HACKATHON.md>, read on 2026-11-03.

| Fact | Value |
| --- | --- |
| Event | NEAR HACKATHON |
| Development starts | 2026-11-01T00:00:00Z |
| Development / submission ends | 2026-11-14T23:59:00Z |
| Judging starts | 2026-11-16T00:00:00Z |
| Results by | 2026-11-18T23:59:00Z |
| Submission | `POST https://nearhackathon.com/api/applications` |
| One submission per project / repository | Yes |
| Repository must be public | By 2026-11-14T23:59:00Z |
| License | Any; MIT is fine; add a LICENSE file |
| Demo post | The human publishes it on X, tags `@NEARProtocol` and `#NEARHackathon` |

What this means:

- The repository may stay private while we build, but it must be public before
  the deadline. **I will not make it public, push, or create a GitHub repo
  without explicit owner confirmation.**
- The human, not the agent, publishes the X post and confirms the submission
  payload. One attempt per repository, so we print the JSON and wait.
- Code written in the window is judged. This repository is new; any library we
  use is disclosed in the README.
- Testnet-only projects are eligible (Q-010). Testnet NEAR is free (Q-008).
- Do not commit secrets. Keys go in environment variables.
- Track ids expected: `defi` (Unstoppable DeFi) and `open` (Open Innovation).
  Confirm against the live form at submission time.

---

## 3. Environment and repository state (VERIFIED)

| Item | Value |
| --- | --- |
| Working directory | `/home/uyscutty/projects/near` |
| Git | not a repository yet |
| Node | `v26.8.1` |
| npm | `11.19.0` |
| cargo | `1.98.1` |
| rustc | `1.98.1` |
| NEAR CLI | `near-cli-rs 0.30.1` at `~/.cargo/bin/near` |
| NEAR testnet RPC | `https://rpc.testnet.near.org` |
| Sepolia chain id | `11155111` |

Library versions checked on 2026-11-03:

| Package / crate | Version |
| --- | --- |
| `near-sdk` | `5.30.0` (MSRV 1.95) |
| `near-workspaces` | `0.24.0` |
| `next` | `16.4.5` |
| `react` | `19.3.1` |
| `wagmi` | `3.7.11` |
| `viem` | `2.57.16` |
| `@near-wallet-selector/core` | `10.1.4` |
| `@near-wallet-selector/modal-ui` | `10.1.4` |
| `@near-js/client` | `2.5.1` |
| `near-api-js` | `7.4.0` |
| `@hot-labs/near-connect` | `0.12.0` |
| `@ethereumjs/block` | `10.1.3` (needed for current Sepolia headers) |
| `@ethereumjs/common` | `10.1.3` |
| `@ethereumjs/trie` | `6.2.1` |
| `@ethereumjs/rlp` | `5.0.2` |

The repository contains only Phase 0 documentation and empty top-level
directories at the time of writing.

---

## 4. NEAR chain signatures (VERIFIED, with one important limitation)

### 4.1 Contracts and keys

| Network | MPC contract |
| --- | --- |
| Mainnet | `v1.signer` |
| Testnet | `v1.signer-prod.testnet` |

Testnet root public key for `domain_id: 0` (secp256k1):

```
secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3
```

Domain ids: `0` = secp256k1 (ECDSA), `1` = Ed25519. The testnet contract does
not expose `latest_key_version`; use domain `0` with the legacy key version.

### 4.2 Deriving a position address (VERIFIED)

`derived_public_key` accepts:

```json
{
  "path": "native-credit:v1:11155111:position:1",
  "predecessor": "native-credit.testnet",
  "domain_id": 0
}
```

Test result:

```
secp256k1:4ha5b44BhvbGUgJvUBtenH8GLZesA2HxiaSjzjR2QfipnCZ8A2EuH2w8TZSLYecCuprDzHpohHLauTgU2XZwXRgn
```

Ethereum address: `0xec29df4cf529cf8be4fbc8380cf732cff82389bc`.

Address derivation: base58-decode the 64-byte `x || y` public key, `keccak256`,
take the last 20 bytes. No secp256k1 decompression is needed if the MPC returns
the uncompressed point.

The predecessor account does not exist yet; the view call still worked. When the
contract calls `sign`, the predecessor must be the deployed contract account, so
the address must be derived from `native-credit.testnet`, not an arbitrary
account.

Path format proposed:

```
native-credit:v1:11155111:position:<position_id>
```

One deposit address per position, not per user. This isolates accounting and
matches the "one in-flight ETH transaction per position" rule.

### 4.3 Signing format (VERIFIED from the deployed source)

The current `sign` call takes:

```json
{
  "request": {
    "path": "native-credit:v1:11155111:position:1",
    "payload_v2": { "Ecdsa": "<hex of the 32-byte signing hash>" },
    "domain_id": 0
  }
}
```

- Attach `1 yoctoNEAR`.
- Attach at least `10 Tgas`.
- The ECDSA payload must be strictly below the secp256k1 field size. Our contract
  must reject malformed hashes before signing, not pass them through.

### 4.4 The limitation: no `verify_foreign_transaction` for Ethereum

`get_available_foreign_chains` on `v1.signer-prod.testnet` returned:

```
Solana, Base, Bnb, Arbitrum, Abstract, Starknet, Polygon, HyperEvm,
Aptos, Sui, Avalanche, Fogo
```

**Ethereum and Sepolia are absent.** Therefore MPC `verify_foreign_transaction`
cannot be used to verify Sepolia deposits or withdrawals. We use the Ethereum
light client instead. This is the main reason Mode A is a separate proof path,
not a chain-signature-native path.

### 4.5 What chain signatures are good for here

- Deriving the deposit address.
- Signing the exact EIP-1559 transaction that the NEAR contract constructs.
- Nothing else. The MPC is not an oracle and does not read Sepolia state.

---

## 5. Sepolia light client and account proofs

### 5.1 Deployed client (VERIFIED)

| Item | Value |
| --- | --- |
| Account | `client-eth2.sepolia.testnet` |
| Version | `0.8.0` |
| Network | Sepolia |
| `last_block_number` encoding | Borsh `u64` little-endian |
| `block_hash_safe(n)` | Borsh `u64` argument; returns Borsh `Option<H256>` (33 bytes when `Some`) |
| `finalized_beacon_block_header()` | Borsh `ExtendedBeaconBlockHeader` (176 bytes) |
| `finalized_beacon_block_root()` | Borsh `H256` |
| `finalized_beacon_block_slot()` | Borsh `u64` |
| `get_client_mode()` | Borsh enum (observed `0`) |
| `initialized()` | Borsh bool |

Observed at 2026-11-03:

```
last_block_number = 12058180  (later 12058242)
block_hash_safe(12058180) = 0x21658d63068ca2abc1329ae88829e17a3ac852c3c52439139b4c43da55b34752
Sepolia finalized block 12058180 hash = 0x21658d63068ca2abc1329ae88829e17a3ac852c3c52439139b4c43da55b34752
```

At block `12058180` the light client's finalized beacon header returned:

```
execution_block_hash = 0x21658d63068ca2abc1329ae88829e17a3ac852c3c52439139b4c43da55b34752
```

and that is exactly the execution block hash on Sepolia for number `12058180`.
**The light client is live and tracking Sepolia finality.**

Note: the deployed `0.8.0` build does **not** expose `get_trusted_signer`,
`get_trusted_blocks_submitter`, `get_hashes_gc_threshold`, or `network`; those
methods are `MethodNotFound` even though they exist in the newer `main` source.
Do not build on them. Use only the methods verified above.

### 5.2 Current Sepolia header format (VERIFIED, important)

Sepolia in November 2026 includes header fields beyond the older Cancun schema:
`requestsHash`, `blockAccessListHash`, and `slotNumber`. The current finalized
header RLP is 688 bytes, not the older ~618-byte encoding.

Verified recipe (off-chain relayer):

```js
import { createBlockFromRPC } from '@ethereumjs/block';
import { Common, Sepolia, Hardfork } from '@ethereumjs/common';
import { keccak256 } from 'ethereum-cryptography/keccak.js';

const common = new Common({
  chain: Sepolia,
  hardfork: Hardfork.Osaka,
  eips: [7928, 7843],
});
const block = createBlockFromRPC(blockRpc, [], { common, setHardfork: true });
const headerRlp = block.header.serialize();
keccak256(headerRlp) === blockRpc.hash; // true
```

At block `12058242`:

```
Sepolia hash          = 0x62d19fa474873932e413f3f0e9cc142d301e5455dc822b9ac80a5426800b896f
keccak(headerRlp)     = 0x62d19fa474873932e413f3f0e9cc142d301e5455dc822b9ac80a5426800b896f
light client          = 0x62d19fa474873932e413f3f0e9cc142d301e5455dc822b9ac80a5426800b896f
```

`@ethereumjs/block` v5 and v10 without EIP-7928/7843 produce a different hash.
**Pin v10.1.3+ and the EIPs explicitly.** The contract must not re-serialize the
header; it must hash the exact RLP bytes and parse only the fields it needs.

### 5.3 Account proof verification (VERIFIED in JavaScript)

At finalized block `12058242`, with `@ethereumjs/trie` v6:

```
address 0x0000000000000000000000000000000000000001
  nonce        0
  balance      0x418c84296add147a
  codeHash     keccak256(empty) = 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
  storageHash  empty trie root  = 0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421
  accountProof 8 nodes
  Trie.verifyProof(keccak256(address), proof) -> non-null account RLP
```

The USDC contract at `0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238` also verified,
with non-empty `codeHash` and `storageHash`. The derived deposit address
`0xec29...89bc` verified as an **absence** proof (non-existent account) at that
block; after it is funded it becomes a normal EOA.

The trie proof call used:

```js
Trie.verifyProof(keccak256(hexToBytes(address)), proofNodes)
```

The account RLP is `[nonce, balance, storageRoot, codeHash]`. For an EOA:

- `codeHash == keccak256("")`
- `storageRoot == 0x56e81f...b421` (empty trie root)

The contract should enforce those two checks before treating the account as a
plain ETH account. If the account is a contract, reject it.

### 5.4 Why the deployed `eth-prover` is not enough (VERIFIED)

`eth-prover.n-bridge.testnet` (version `2.0.0`) and `prover.sepolia.testnet`
expose `verify_log_entry` and `verify_storage_proof`. `verify_storage_proof`
does verify the account proof, but then immediately verifies a **storage proof**:

```rust
let account_state = Self::verify_trie_proof(header.state_root, account_key, account_proof);
let storage_hash: H256 = Rlp::new(&account_state).val_at(2).unwrap();
let storage_value = Self::verify_trie_proof(storage_hash, storage_key_hash, storage_proof);
```

It indexes `proof[proof_index]`, so an empty storage proof panics. An EOA has an
empty storage trie, and asking for a storage slot returns an empty proof. There
is no EOA-only method. **We need a small account-proof verifier of our own.**

### 5.5 Contract-side verifier design (TO PROVE)

The safe design avoids typed header structs that drift with Ethereum forks:

1. Relayer supplies `header_rlp: Vec<u8>` and `account_proof: Vec<Vec<u8>>`.
2. Contract hashes the **exact** `header_rlp` with keccak256.
3. Contract parses the RLP list generically:
   - field `3` = `state_root` (32 bytes)
   - field `8` = `number` (big-endian integer, must fit `u64`)
   - require the list has at least 9 fields and the expected shapes.
4. Contract calls `client-eth2.sepolia.testnet.block_hash_safe(number)` with a
   bounded gas amount.
5. In the callback (private, predecessor must be the contract):
   - compare the returned `Option<H256>` with the local header hash;
   - if equal, verify the account proof against `state_root` for
     `keccak256(deposit_address)`;
   - decode the account RLP, require EOA (`codeHash`, empty `storageRoot`);
   - record the balance/nonce/block snapshot and credit the delta.
6. Reject if `number <= last_verified_block` for the position (no replayed or
   older proofs), or if the proof was already used.

This design depends only on the RLP field positions, which are stable, and on
`block_hash_safe`, which the light client guarantees. It does not depend on the
contract knowing about EIP-7928/7843. The off-chain relayer is responsible for
building the canonical header RLP. If it builds the wrong bytes, the hash check
fails, which is safe.

Open questions for Phase 1:

- Gas cost of the trie proof and the callback on NEAR testnet. **TO MEASURE.**
- Whether the RLP proof nodes fit in a single transaction's arguments and gas
  budget. **TO MEASURE.**
- Whether we need a separate verifier contract or an internal module. Start
  internal; split only if the main contract gets too large.
- The light client's `hashes_gc_threshold` is unknown on `0.8.0` (the getter is
  absent). Use recent finalized blocks; do not rely on old proofs.

### 5.6 Proof-provider quorum (TO PROVE)

The relayer should fetch `eth_getBlockByNumber("finalized")` and
`eth_getProof(address, [], blockNumber)` from at least two independent providers
and require identical block hash, state root, balance, nonce, and account proof
root. If they disagree, do not submit; retry or alert.

Verified providers on 2026-11-03:

| Provider | `eth_getProof` at finalized | Notes |
| --- | --- | --- |
| `https://sepolia.rpc.thirdweb.com` | works | currently the primary; no key needed |
| `https://eth-sepolia.api.onfinality.io/public` | works | rate-limited without a key |
| `https://ethereum-sepolia-rpc.publicnode.com` | fails | `distance to target block exceeds maximum proof window` |
| `https://sepolia.gateway.tenderly.co` | fails | same proof-window error |
| `https://api.zan.top/...` | fails | same proof-window error |
| `https://1rpc.io/sepolia` | fails | historical state not available |
| `https://sepolia.drpc.org` | requires paid plan | not usable |
| `https://rpc.sepolia.org` | fetch failed | not reliable |

**Need:** an Alchemy or Infura Sepolia key in an environment variable for a
second reliable provider. Ask the owner. The relayer must never hard-code a key.

---

## 6. Testnet USDC (VERIFIED)

Source: Circle USDC contract addresses,
<https://developers.circle.com/stablecoins/usdc-contract-addresses>.

| Item | Value |
| --- | --- |
| NEAR testnet USDC contract | `3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af` |
| Standard | NEP-141 |
| Symbol | `USDC` |
| Decimals | `6` |
| Total supply observed | `14081220210367770` (raw units) |
| Storage bound observed | `1250000000000000000000` yoctoNEAR |

Funding routes:

- **Circle faucet** <https://faucet.circle.com/> supports NEAR Testnet,
  no account required, `20 USDC` per address per chain every 2 hours. The
  recipient must be a NEAR testnet account. **TO PROVE:** whether the faucet
  automatically registers storage for the recipient, or whether we must call
  `storage_deposit` first.
- NEAR testnet Ref Finance can swap testnet NEAR for testnet USDC; liquidity may
  be low.
- NEAR faucet <https://near-faucet.io/> and the NEAR docs faucet provide testnet
  NEAR, not USDC.

If the real USDC route fails during the demo, the fallback is a mock NEP-141
contract deployed by us, clearly labeled `SIMULATED USDC` in the UI and README.
Do not silently substitute it.

---

## 7. Price oracle (VERIFIED stale, plan needed)

### 7.1 Pyth is not usable

- Pyth Core stopped supporting NEAR after 2026-08-26.
- `pyth-oracle.testnet.get_price` returns `null`.
- `get_price_no_older_than` returned an ETH price of `156384000000` with
  exponent `-8`, `publish_time = 1780781716` (`2026-06-06`), roughly 150 days
  stale on 2026-11-03.
- A stale price is unsafe for lending and liquidation.

### 7.2 Outlayer / `price-oracle.testnet`

`price-oracle.testnet` exists but `get_price` and `get_ema_price` panic. The
Outlayer docs use `oracle_call` / `request_price_data` with a `0.02 NEAR`
deposit and a callback; the mainnet contract is `price-oracle.near`.
`priceoracle.near` is `UNKNOWN_ACCOUNT`. This path is **not verified** for
testnet and is likely a request/response flow, not a simple view.

### 7.3 Proposed oracle plan

**Recommended for the prototype:** a clearly labeled quorum price signer.

- 3 signer keys, 2-of-3 threshold.
- Each update: `{ pair, price, exponent, timestamp, sequence }`.
- Contract verifies signatures, requires increasing sequence, rejects stale
  timestamps (for example > 5 minutes), and stores the median.
- We run the signers; the UI and README label the feed as `PARTLY SIMULATED`
  or `TEAM-OPERATED`, not as a decentralized oracle.
- Fallback: a single demo oracle key, clearly labeled `DEMO ORACLE`.

The contract must never accept a price older than a configured maximum age for
new loans or liquidations. If the feed is stale, new borrowing stops; repayment
and liquidation handling must still be safe and documented.

---

## 8. Reference products and design lessons (VERIFIED, no code copied)

- **Pusaka** <https://github.com/PugarHuda/pusaka>: NFTs holding native BTC via
  chain signatures, uses `btc-client-v4.testnet` and `pusaka-btc.testnet`.
  Confirms the chain-signature pattern and that NEAR price oracles were stale.
  Ideas only; no code reuse.
- **Omni Bridge / eth-light-client-contract**
  <https://github.com/Near-One/eth-light-client-contract>: the `eth-prover`
  proof pattern and the `block_hash_safe` call are the reference for Mode A.
  The repository has no root LICENSE file, so do not copy code verbatim;
  implement the small verifier independently.
- **Aave v3.3**: when collateral is zero and debt remains, the remaining debt is
  burned and a deficit is accounted to the reserve. This is the reference for
  bad-debt handling; our prototype should at least record a deficit rather than
  silently zeroing debt.
- **Aave native BTC market proposal (Babylon Labs)**: native BTC stays in a
  vault and Aave uses restricted internal ERC-20 accounting; liquidation still
  needs a fallback asset. This is close to our custody model but not the same;
  do not claim it is Aave.
- **THORChain loans**: pooled/rebasing model, different from this
  overcollateralized position model. Use it only as a contrast in the README.

---

## 9. Proposed architecture

### 9.1 Components

```
contracts/native-credit/      NEAR lending + proof verifier
contracts/credit-oracle/      optional quorum price signer verifier
agents/relayer/               fetch proofs, submit deposits/confirmations,
                              broadcast signed Sepolia transactions
agents/watcher/               independent read-only proof checker + alerts
agents/keeper/                health-factor monitor + liquidation candidates
agents/liquidator/            repay USDC, claim liquidation tickets
web/                          Next.js + React UI, NEAR wallet + Sepolia wallet
scripts/                      deploy, fund, demo, proof-generation helpers
docs/                         runbooks, demo script, threat model
```

### 9.2 Position state machine

```
DerivingAddress
  -> Idle
  -> WithdrawalPending  (one in-flight ETH tx)
  -> Idle
  -> LiquidationPending (one in-flight ETH tx)
  -> Closed
```

`DerivingAddress`: position created; MPC `derived_public_key` callback not yet
returned. No deposits or borrows accepted.

`Idle`: address derived; deposits can be verified; borrows allowed if healthy
and risk-off is off.

`WithdrawalPending`: contract has debited/reserved the amount and requested a
signature; no new borrows, no new withdrawals, no liquidation of this position
until the pending tx resolves. Deposits during this window are not credited
automatically; they are flagged for review.

`LiquidationPending`: a liquidator has repaid and the contract is signing the
collateral transfer; no new actions.

`Closed`: no debt and no collateral, or liquidated and settled.

### 9.3 Deposit flow

1. User creates a position. Contract calls MPC `derived_public_key` with
   `path = native-credit:v1:11155111:position:<id>`, `predecessor = contract`,
   `domain_id = 0`.
2. Callback stores the derived public key and computed Ethereum address.
3. User sends Sepolia ETH to that address.
4. Relayer fetches a finalized header + EIP-1186 account proof from two
   providers, checks they agree, and calls `record_deposit`.
5. Contract verifies the header hash via `block_hash_safe`, verifies the account
   proof, requires an EOA, and credits `new_balance - last_verified_balance`.
6. Contract updates `last_verified_block`, `last_verified_balance`,
   `last_verified_nonce`.

No deposit is credited before the block is finalized by the light client.

### 9.4 Withdrawal flow

1. User requests a withdrawal to a registered Sepolia address.
2. Contract requires `amount > max_gas_cost` and
   `amount + max_gas_cost <= available_collateral`.
3. Contract builds an EIP-1559 transaction:
   `chainId = 11155111`, `nonce = last_verified_nonce`, `to = registered address`,
   `value = amount`, empty data, fixed gas limit, bounded fees.
4. Contract computes the EIP-1559 signing hash and calls MPC `sign` with
   `payload_v2: { Ecdsa: hash }`, `domain_id = 0`, `1 yoctoNEAR`, `10 Tgas`.
5. Callback parses `big_r`, `s`, `recovery_id`, checks `r`/`s` ranges, recovers
   the signer address (or checks it with a small secp256k1 recovery library),
   requires it to equal the stored deposit address, assembles the raw
   transaction, and stores it.
6. Relayer broadcasts the raw transaction. The contract records
   `tx_hash = keccak256(raw_tx)` and waits.
7. After finality, relayer submits a new account proof. Contract requires the
   nonce advanced and decides:
   - success: `old_balance - new_balance >= amount` and
     `<= amount + max_gas_cost`;
   - failure: the delta is smaller than `amount` (gas-only spend); the reserved
     amount is released, and the gas loss stays with the position.
8. Contract sets `last_verified_balance = new_balance`, clears the pending
   operation, and returns to `Idle`.

Because the contract cannot see the receipt status, the success/failure test is
a balance-delta test. It is only unambiguous if no unsolicited ETH arrives while
the withdrawal is pending and `amount > max_gas_cost`. The UI must warn users
not to send ETH to a position with a pending withdrawal. If the delta is
ambiguous, the contract must **not** guess: mark the operation `NeedsReview` and
require an operator decision that cannot move collateral arbitrarily.

### 9.5 Liquidation flow

1. Keeper/watcher computes health from the latest oracle price and the verified
   collateral (minus pending outflow).
2. Liquidator repays USDC via `ft_transfer_call` (or an internal repay method).
3. Contract creates a liquidation ticket/claim:
   `{ position_id, liquidator, collateral_amount, discount, sepolia_address }`.
4. Contract requests a chain signature for the exact ETH transfer to the
   liquidator's registered Sepolia address, under the same one-in-flight rule.
5. After finality, the proof confirms the outflow and the position is settled.
6. If the collateral is insufficient to cover the debt, record a deficit
   (bad-debt) rather than silently zeroing it.

Repayment and liquidation must remain callable even when new borrowing is paused
or the contract is in risk-off mode.

### 9.6 Risk-off mode

- Admin or guardian can enable risk-off.
- Effect: stop new borrows; optionally raise the collateral requirement for new
  positions.
- It must not stop repayment, liquidation, or the settlement of existing
  operations.
- Admin cannot move collateral, change a position's deposit address, or sign an
  arbitrary transaction.
- Parameter changes should be timelocked or at least logged and visible in the
  UI. Confirm the spec requirement before implementing a timelock.

### 9.7 Oracle integration

- `credit-oracle` verifies 2-of-3 signed price updates and exposes a view.
- The lending contract stores the last accepted price and timestamp.
- New borrows require a fresh price.
- Liquidations require a fresh price; if stale, the UI shows "liquidation
  paused by oracle staleness" and the contract refuses to liquidate at an
  unknown price. This is a trade-off: refusing to liquidate also creates bad
  debt. Document the chosen rule and test it.

### 9.8 USDC handling

- Use the real Circle testnet USDC contract.
- Require users to register with USDC (NEP-145 `storage_deposit`).
- `ft_on_transfer` must check `predecessor_account_id == USDC_CONTRACT` and
  return the full amount for any unexpected token or malformed message.
- Use `ft_transfer` + a callback for outgoing USDC so a failed transfer does not
  silently leave the loan recorded but undelivered.
- Test borrow, repay, liquidation repayment, and wrong-token rejection.

### 9.9 Frontend

- Next.js 16 + React 19.
- NEAR wallet selector / near-connect for NEAR.
- wagmi + viem for Sepolia reads and wallet transactions.
- Show: position id, deposit address, QR, verified balance, last verified block,
  health factor, debt, pending operation, oracle price/age, risk-off state, and
  labels (`REAL TESTNET`, `PARTLY SIMULATED`, `DEMO`).
- Do not hide the trust assumptions in a footer; put them on the position page.

---

## 10. Trust model and invariants

### 10.1 Trust assumptions to disclose

| Component | Trust assumption |
| --- | --- |
| `client-eth2.sepolia.testnet` | We trust the deployed light client and its finality updates. It is external infrastructure, not our code. |
| `v1.signer-prod.testnet` | We trust the MPC threshold network for key derivation and signatures. Testnet only. |
| Sepolia RPCs | We require two providers to agree; a single lying provider must not be enough. |
| Price oracle | Team-operated quorum; clearly labeled. Not a decentralized oracle. |
| Admin/guardian | Can change risk parameters / risk-off, but cannot move collateral or sign arbitrary transactions. |
| Relayer | Can censor or delay, but cannot forge proofs or alter signed transactions. |
| USDC | We trust Circle's testnet NEP-141 contract. |
| NEAR RPC | Liveness/read correctness for the frontend; contracts use their own calls. |

### 10.2 Invariants to test

1. No ETH leaves a position except to its registered withdrawal address or a
   registered liquidation address.
2. No signature is requested for a transaction the contract did not construct.
3. At most one in-flight ETH transaction per position.
4. A deposit is never credited before finality.
5. A withdrawal is never marked complete before finality.
6. Collateral accounting cannot exceed the last verified on-chain balance minus
   pending outflows.
7. Liquidation and repayment cannot be paused by admin/risk-off.
8. Admin cannot transfer collateral or change a deposit address.
9. Wrong FT contracts are rejected and the full amount is returned.
10. Integer math never overflows/underflows; all health-factor math is checked.
11. A replayed or older account proof cannot credit twice.
12. A proof for a non-EOA account is rejected.
13. A stale oracle price cannot open a new loan.
14. Removing any one guard makes a specific test fail (mutation test).

---

## 11. Costs and funding

| Item | Status |
| --- | --- |
| NEAR testnet account creation | Free via faucet / near-cli-rs; **TO DO** |
| NEAR contract deploy/storage | Testnet NEAR; measure actual gas/storage |
| MPC `sign` fee | 1 yoctoNEAR deposit + gas; measure actual |
| Sepolia ETH for test users | Faucets (Alchemy, Infura, QuickNode, Chainlink); need owner |
| Sepolia gas for withdrawals | Paid from the position's ETH; requires `amount > max_gas_cost` |
| USDC | Circle faucet, 20 USDC / 2h / address / chain; no real value |
| Hosting | Local demo acceptable; hosted demo is a plus |

The Sepolia gas cost must be measured during Phase 1. The contract must not
allow a withdrawal so small that gas can consume the entire amount and make the
success/failure test ambiguous.

---

## 12. Risks and open decisions

### 12.1 Risks

1. **Header schema drift.** Sepolia already has EIP-7928/7843 fields. The
   generic-RLP design avoids typed structs, but the relayer's header encoder must
   be pinned and tested against `block_hash_safe` on every run.
2. **On-chain trie proof gas/size.** Not yet measured. This is the main
   technical risk for Mode A.
3. **RPC redundancy.** Only one reliable keyless provider today. Need an
   Alchemy/Infura key.
4. **Oracle.** No usable real price feed. A team-operated quorum is a trust
   assumption, not a decentralized oracle.
5. **Withdrawal reconciliation.** Balance-delta success detection is ambiguous
   if unsolicited ETH arrives during a pending withdrawal. Lock the position and
   flag ambiguity instead of guessing.
6. **Signature verification.** Recovering the signer on-chain needs a
   wasm-friendly secp256k1 library and gas; not yet tested.
7. **USDC delivery.** Outgoing `ft_transfer` needs a callback; test the failure
   path.
8. **MPC testnet availability/latency.** Measure and add retries/timeouts.
9. **Time.** Today is 2026-11-03; the deadline is 2026-11-14. Mode A is
   feasible only with a focused scope and no detours.

### 12.2 Decisions needed from the owner

1. **Approve Mode A** (light client + EIP-1186 account proofs) and the custom
   verifier, or choose Mode B (attested relayer, clearly labeled).
2. **Provide Sepolia RPC keys** (Alchemy/Infura) for a second reliable proof
   provider, via environment variables.
3. **Approve the oracle plan**: 2-of-3 team signer with a clear label, or a
   single demo oracle.
4. **Approve creating `native-credit.testnet` and deploying to testnet.**
   Testnet only; no mainnet.
5. **Confirm the liquidation ticket representation**: internal claim record vs.
   NEP-171 NFT. The spec wording matters.
6. **Confirm the repo policy**: keep local/private until the owner approves a
   push/public repo.
7. **Confirm the exact spec details** that are not in this research document
   (risk parameters, loan terms, ticket rules, pause rules, demo scope).

---

## 13. Phase plan

### Phase 0 - Research (this document)

- Done: rules, MPC, light client, proofs, RPCs, USDC, oracles, references.
- Stop: owner reads and approves.

### Phase 1 - Local proof of concept

- Scaffold the repository and workspace.
- Implement the NEAR contract with mock light client / mock MPC for unit tests.
- Implement and test the generic RLP header parser + account proof verifier
  against captured Sepolia vectors.
- Implement position creation, deposit recording, borrow/repay math, and
  liquidation math with property tests.
- Implement `ft_on_transfer` guards, storage registration, and wrong-token
  refund tests.
- Run `cargo test`, `cargo clippy`, and `near-workspaces` tests.

### Phase 2 - Real testnet integration

- Create `native-credit.testnet`; deploy.
- Wire the real light client `client-eth2.sepolia.testnet`.
- Wire the real MPC `v1.signer-prod.testnet` for derivation and signing.
- Build the relayer: header + proof generation, provider quorum, submission.
- Build the withdrawal signer + broadcaster + finality reconciliation.
- Build the watcher, keeper, and liquidator.
- Verify one real Sepolia deposit and one real withdrawal end to end.

### Phase 3 - Product and submission

- Frontend: position page, deposit QR, health factor, pending states, risk-off,
  oracle age, labels.
- Demo script and video.
- README for a machine reader and a human reader.
- Mutation tests: remove each guard, show the failing test.
- Threat model and limitations.
- Submission payload after owner confirmation.

---

## 14. Sources

- NEAR HACKATHON rules: <https://nearhackathon.com/HACKATHON.md>
- NEAR chain signatures: <https://docs.near.org/chain-abstraction/chain-signatures>
- MPC testnet contract: `v1.signer-prod.testnet` (queried live)
- Ethereum light client: <https://github.com/Near-One/eth-light-client-contract>
- Deployed light client: `client-eth2.sepolia.testnet` (queried live)
- Circle USDC addresses:
  <https://developers.circle.com/stablecoins/usdc-contract-addresses>
- Circle faucet: <https://faucet.circle.com/>
- NEAR faucet: <https://near-faucet.io/>
- NEAR docs faucet: <https://docs.near.org/getting-started/faucet>
- Sepolia RPC providers: <https://docs.near.org/api/rpc/providers> (NEAR) and
  the individual Sepolia endpoints listed above
- Pusaka: <https://github.com/PugarHuda/pusaka>
- Aave v3.3 bad debt: Aave documentation / v3.3 release notes
- Aave native BTC proposal: Babylon Labs / Aave governance discussion
- THORChain loans: THORChain documentation
- `@ethereumjs/block`, `@ethereumjs/common`, `@ethereumjs/trie`: npm packages

---

## 15. Reproduction notes

The temporary research scripts are in `/tmp/opencode`:

| Script | What it does |
| --- | --- |
| `rpc.mjs`, `rpc2.mjs` | Query MPC and light client; Borsh decode |
| `derive.mjs` | Test `derived_public_key` paths and addresses |
| `sep.mjs`, `prov.mjs`, `prov2.mjs` | Sepolia RPC/finality/`eth_getProof` tests |
| `verifyproof4.mjs` | EIP-1186 account proof verification with `@ethereumjs/trie` |
| `usdc.mjs` | USDC metadata/supply |
| `oracle.mjs` | Pyth/Outlayer oracle queries |
| `eth2lib.rs`, `ethprover.rs`, `eth2types.rs`, `ethtypes-lib.rs` | Fetched source snapshots for reading |
| `HACKATHON.md` | Fetched hackathon rules |

No secrets are stored in these files. The temporary `node_modules` may be
deleted at any time; the recipe in section 5.2 is the important part.
