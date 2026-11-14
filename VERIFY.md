# VERIFY.md

How to reproduce the Phase 0 research claims and the Phase 1 local contract
checks. No secrets are needed for these checks.

Working directory for the temporary research scripts: `/tmp/opencode`.

## 0. Local contract checks

Run from `/home/uyscutty/projects/near`:

```sh
cargo test -p native-credit
cargo clippy -p native-credit --all-targets -- -D warnings
cargo build -p native-credit --target wasm32-unknown-unknown --release
cargo test -p native-credit --test local_e2e
```

Expected: 38 tests pass (36 unit + 2 local E2E), clippy is clean, and
`target/wasm32-unknown-unknown/release/native_credit.wasm` is produced.
The upgraded `v2.native-credit.testnet` wasm is `609491` bytes, sha256
`345e1c91175345e78fc5b49c4ea6b9b18f06ed9de7dd3fbb9447264b7f330efd`,
with code hash `4XRS7nEZJqD6JC5zbE35xMn2gG1D1PVi7yZrChHjxx3N`. The previous
deployed v2 was `605982` bytes, code hash
`8R8LwzDfAtcLipoAiyyxfLsKhvJC8QWoGpiCpkKSBznR`; the retired v1 contract ran
the earlier `601264`-byte wasm.

The contract tests use captured real Sepolia data:

- `testdata/sepolia_12058306.json` - finalized header RLP, state root, EOA
  account proof, contract account proof, and absence proof.
- `testdata/sepolia_tx_12058498.json` - a real finalized Sepolia EIP-1559
  transfer. `crypto::assemble_signed_tx` reproduces the exact raw transaction
  and hash, and signature recovery returns the real sender.

The key local assertions:

- withdrawal gas validation requires at least 250,000, above the ~207,000 EIP-8037 fresh-account cost; live v2 now uses 250,000;
- exact header hash/state root and proof verification;
- contract accounts rejected, EOA accounts accepted;
- absence proof accepted;
- deposit is credited only after the light client returns the matching hash;
- mismatched finality hash does not credit;
- MPC-style signature from the position key builds a valid raw transaction;
- signature from any other key cancels the withdrawal;
- real EIP-1559 raw transaction bytes and sender recovery match Sepolia;
- `tests/local_e2e.rs` drives create, derive, deposit, borrow, repay, withdraw,
  confirm, and liquidation with synthetic single-leaf MPT proofs and mocked MPC
  signatures/light-client hashes;
- `get_positions_by_owner` discovers positions from the existing ID range without
  changing contract state layout.

## 0a. Deployed contract

```sh
python3 - <<'PY'
import json, urllib.request, base64
def view(method, args={}):
    body = json.dumps({"jsonrpc":"2.0","id":1,"method":"query","params":{
        "request_type":"call_function","finality":"final",
        "account_id":"v2.native-credit.testnet","method_name":method,
        "args_base64":base64.b64encode(json.dumps(args).encode()).decode()}}).encode()
    r = json.load(urllib.request.urlopen(urllib.request.Request(
        "https://rpc.testnet.near.org", data=body,
        headers={"content-type":"application/json"}), timeout=30))
    return bytes(r["result"]["result"]).decode()
print(view("get_config"))
print(view("get_price"))
print(view("get_position", {"position_id": 1}))
PY
```

Expected: owner `v2.native-credit.testnet`, guardian `native-credit.testnet`,
`gas_limit=250000`, the three Ed25519 oracle keys with threshold `2`, and
position `1` owned by `borrower.native-credit.testnet` with `deposit_address`
`0x81163cfa3c95b3691fdb4fb0766ecafcfa267999`. Final live position `1` is
`Idle` with `collateral_wei=3897999998392000`, `debt_usdc=0`, no pending
operations, `last_verified_nonce=3`, `last_verified_block=12080729`, and a
`Settled` liquidation ticket. Oracle sequence is `6` at `$3000` ETH/USD.
The contract holds `20 USDC` after the deposit/borrow/repay/liquidate cycle.

## 0b. Relayer checks

Run from `/home/uyscutty/projects/near/agents/relayer`:

```sh
npm install --no-audit --no-fund
npm test
npm run proof --   --address 0x0000000000000000000000000000000000000001   --position 1   --out /tmp/deposit-proof.json   --args-out /tmp/deposit-args.json
```

Expected: 24 tests pass. The proof command uses the public Sepolia providers,
requires a 2-provider quorum, writes the contract arguments, and prints a block
number. With the default light-client preflight, the output's
`light_client.matches` must be `true`. If the account is absent, the command
warns and still writes a valid absence proof; regenerate after funding.

On 2026-11-07 the default public providers rejected pinned finalized proofs
with `distance to target block exceeds maximum proof window`; thirdweb returned
HTTP 403. For the live run, use OnFinality with `--min-quorum 1` and an explicit
`--block <n>` that the NEAR light client also reports safe:

```sh
npm run proof -- \
  --address 0x81163cfa3c95b3691fdb4fb0766ecafcfa267999 \
  --position 1 \
  --providers https://eth-sepolia.api.onfinality.io/public \
  --min-quorum 1 \
  --block <safe-block> \
  --out /tmp/deposit-proof.json \
  --args-out /tmp/deposit-args.json
```

Single-provider quorum is a live-workaround, not the production trust model.

For the live position, replace the example address with
`0x81163cfa3c95b3691fdb4fb0766ecafcfa267999` and keep `--position 1`. The
submit builder normalizes a bare numeric deposit to `NEAR`; the real command
must show `attached-deposit 0NEAR`.

The generated `/tmp/deposit-args.json` is the JSON argument object for
`record_deposit`. The full proof file contains metadata and the same object
under `contract_args`. Preview the submission command without sending it:

```sh
npm run submit -- \
  --contract v2.native-credit.testnet \
  --args /tmp/deposit-args.json \
  --signer relayer.testnet \
  --dry-run
```

To check the signed withdrawal broadcast path without spending gas, run it
against a deployed contract after a withdrawal has been signed:

```sh
npm run broadcast -- --contract v2.native-credit.testnet --position 1 --dry-run
```

Live v2 withdrawal evidence:

- Fresh payout `0x0d5d...b8fa6`:
  `0x66a3a1e2e4cb7a5d70ec55ac10a0fea97255bf077035a04ef2396c70fd7fc74d`
  failed at block `12080523` with `gasUsed=60000` (status `0x0`). Sepolia
  estimates ~`207391` gas because EIP-8037 charges `183600` state gas to create
  the account.
- Existing payout `0xec29...89bc`:
  `0x0ce9ccdf7abacfd77b4d4569e46ea3dcefb512034a66d68b4e180b42d2d4ca40`
  succeeded at block `12080627` with `gasUsed=21000`.

The live v2 contract was then upgraded in place and its gas limit raised to
`250000` (`set_gas_limit` tx
`FF9QM6yfBFhHTheWvUuT5W9cJe4KStnPjvqUTHz9cRLp`). The fresh-EOA retry is pending
a top-up above the `0.01 ETH` minimum. See `AUDIT/FOLLOWUP.md`.

## 0c. Oracle checks

```sh
cd /home/uyscutty/projects/near/agents/oracle
npm test
ORACLE_KEYS_FILE="$HOME/.near-credentials/native-credit-oracle-keys.json" \
  npm run update -- --contract v2.native-credit.testnet --eth-usd 3000 --usdc-usd 1.00 --dry-run
```

Expected: 4 tests pass, `signatures 3/2`, and a `near-cli` command with
`attached-deposit 0NEAR`. The signer reads `get_config().oracle_keys`, signs the
exact message
`native-credit:v1:price:{eth_usd_1e8}:{usdc_usd_1e8}:{timestamp}:{sequence}`,
and fills missing key slots with empty strings. Private seeds never leave
`~/.near-credentials/native-credit-oracle-keys.json`.

## 0d. Frontend checks

```sh
cd /home/uyscutty/projects/near/web
npm install --no-audit --no-fund
npm run build
npm run dev
```

Expected: the production build succeeds. The dashboard still contains v1 demo
copy, but it now defaults to `v2.native-credit.testnet`. Position `1` renders
with `0.003897999998392` ETH collateral, zero debt, no pending operation,
oracle sequence `6`, and the v2 risk parameters. The dashboard labels the
light-client check as real, the team oracle as partly simulated, and test USDC
as simulated.

## 0e. Agent checks

```sh
cd /home/uyscutty/projects/near/agents/watcher
npm install --no-audit --no-fund
npm test
npm run watch -- --contract v2.native-credit.testnet --positions 1 --json

cd ../keeper
npm test
npm run keeper -- --contract v2.native-credit.testnet --positions 1 --json

cd ../liquidator
npm test
npm run liquidate -- \
  --contract v2.native-credit.testnet \
  --usdc 3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af \
  --position 1 --json
```

Expected: watcher `16` tests, keeper `7` tests, liquidator `10` tests. Position
`1` reports `Idle`, infinite health, zero debt, `0.003897999998392` ETH
collateral, and a fresh oracle. The liquidator dry run exits with
`health ∞ is not below 1.00x`; it never sends without `--execute`.

## 1. NEAR MPC derivation

```sh
curl -sS https://rpc.testnet.near.org \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc":"2.0","id":1,"method":"query","params":{
      "request_type":"call_function","finality":"final",
      "account_id":"v1.signer-prod.testnet",
      "method_name":"derived_public_key",
      "args_base64":"eyJwYXRoIjoibmF0aXZlLWNyZWRpdDp2MToxMTE1NTExMTpwb3NpdGlvbjoxIiwicHJlZGVjZXNzb3IiOiJ2Mi5uYXRpdmUtY3JlZGl0LnRlc3RuZXQiLCJkb21haW5faWQiOjB9"
    }
  }'
```

Expected: a base58 secp256k1 public key. The corresponding Ethereum address for
v2 position `1` is `0x81163cfa3c95b3691fdb4fb0766ecafcfa267999`.

## 2. Light client liveness and hash match

Run:

```sh
cd /tmp/opencode
node rpc2.mjs
```

Expected: `last_block_number`, `block_hash_safe(last)` and the version. Compare
`block_hash_safe(last)` with Sepolia's finalized block hash.

`block_hash_safe` takes Borsh bytes, not JSON: encode the block number as an
8-byte little-endian `u64` and base64 it. Passing a JSON number silently
deserializes the wrong value and returns `None`. The relayer's
`src/light-client.mjs` does this correctly.

```sh
curl -sS https://sepolia.rpc.thirdweb.com \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["finalized",false]}'
```

The two hashes must match.

## 3. Current Sepolia header RLP

```sh
cd /tmp/opencode
node --input-type=module -e "
import { createBlockFromRPC } from '@ethereumjs/block';
import { Common, Sepolia, Hardfork } from '@ethereumjs/common';
import { keccak256 } from 'ethereum-cryptography/keccak.js';
import { bytesToHex } from '@ethereumjs/util';
const b = await (await fetch('https://sepolia.rpc.thirdweb.com', {
  method:'POST', headers:{'content-type':'application/json'},
  body: JSON.stringify({jsonrpc:'2.0',id:1,method:'eth_getBlockByNumber',params:['finalized',false]})
})).json().then(j => j.result);
const common = new Common({chain:Sepolia, hardfork:Hardfork.Osaka, eips:[7928,7843]});
const block = createBlockFromRPC(b, [], {common, setHardfork:true});
const rlp = block.header.serialize();
console.log(parseInt(b.number,16), b.hash, bytesToHex(keccak256(rlp)), rlp.length);
"
```

Expected: the RPC hash and `keccak256(rlp)` are identical; the RLP length is
roughly 650-700 bytes at the time of writing.

## 4. EIP-1186 account proof

```sh
cd /tmp/opencode
node --input-type=module -e "
import { Trie } from '@ethereumjs/trie';
import { keccak256 } from 'ethereum-cryptography/keccak.js';
import { bytesToHex, hexToBytes } from '@ethereumjs/util';
const S='https://sepolia.rpc.thirdweb.com';
const q=async(m,p)=>{const r=await fetch(S,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:m,params:p})});const j=await r.json();if(j.error)throw new Error(JSON.stringify(j.error));return j.result};
const f=await q('eth_getBlockByNumber',['finalized',false]);
const a='0x0000000000000000000000000000000000000001';
const p=await q('eth_getProof',[a,[],f.number]);
const ok=await Trie.verifyProof(keccak256(hexToBytes(a)), p.accountProof.map(x=>hexToBytes(x)));
console.log('block',parseInt(f.number,16),'nonce',p.nonce,'balance',p.balance,'proofNodes',p.accountProof.length,'accountRlp',ok?bytesToHex(ok):null);
"
```

Expected: a non-null account RLP. For address `0x...01`, the code hash is
`keccak256("")` and the storage root is the empty trie root.

## 5. USDC metadata

```sh
cd /tmp/opencode
node usdc.mjs
```

Expected: `name`/`symbol` `USDC`, `decimals` `6`, the contract account
`3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af`.

## 6. Oracle staleness

```sh
cd /tmp/opencode
node oracle.mjs
```

Expected: `pyth-oracle.testnet` returns `null` or a price with
`publish_time` around `1780781716` (2026-06-06). `price-oracle.testnet`
`get_price` panics. Do not use these as a live price feed.

## 7. Proof-provider matrix

```sh
cd /tmp/opencode
node prov2.mjs
```

Expected on 2026-11-07: OnFinality returns a valid pinned finalized proof;
thirdweb returns HTTP 403; publicnode, tenderly, and zan return
`distance to target block exceeds maximum proof window`; 1rpc, blockpi, and
drpc fail with historical-state or HTTP errors; ethpandaops rate-limits. Re-test
before relying on a provider.
