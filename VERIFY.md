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
```

Expected: 31 tests pass, clippy is clean, and
`target/wasm32-unknown-unknown/release/native_credit.wasm` is produced.
The current wasm is `601264` bytes, sha256
`ca03505b5e6aa23601cbaa46848eecdf74d24b2be366b0baa48312e295d6f143`,
with code hash `EbaHaLfHMuNuoSGhWBfMMJtKqBjhruryaZ31YJwVJu1C`; redeploying
identical code produces the same hash.

The contract tests use captured real Sepolia data:

- `testdata/sepolia_12058306.json` - finalized header RLP, state root, EOA
  account proof, contract account proof, and absence proof.
- `testdata/sepolia_tx_12058498.json` - a real finalized Sepolia EIP-1559
  transfer. `crypto::assemble_signed_tx` reproduces the exact raw transaction
  and hash, and signature recovery returns the real sender.

The key local assertions:

- exact header hash/state root and proof verification;
- contract accounts rejected, EOA accounts accepted;
- absence proof accepted;
- deposit is credited only after the light client returns the matching hash;
- mismatched finality hash does not credit;
- MPC-style signature from the position key builds a valid raw transaction;
- signature from any other key cancels the withdrawal;
- real EIP-1559 raw transaction bytes and sender recovery match Sepolia.

## 0a. Deployed contract

```sh
python3 - <<'PY'
import json, urllib.request, base64
def view(method, args={}):
    body = json.dumps({"jsonrpc":"2.0","id":1,"method":"query","params":{
        "request_type":"call_function","finality":"final",
        "account_id":"native-credit.testnet","method_name":method,
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

Expected: owner/guardian `native-credit.testnet`, the three Ed25519 oracle keys
with threshold `2`, and position `1` owned by
`borrower.native-credit.testnet` with `deposit_address`
`0xec29df4cf529cf8be4fbc8380cf732cff82389bc`. Position `1` has
`collateral_wei=50000000000000000`, `debt_usdc=0`, and a pending `0.01` ETH
withdrawal until its Sepolia confirmation is submitted.

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

For the live position, replace the example address with
`0xec29df4cf529cf8be4fbc8380cf732cff82389bc` and keep `--position 1`. The
submit builder normalizes a bare numeric deposit to `NEAR`; the real command
must show `attached-deposit 0NEAR`.

The generated `/tmp/deposit-args.json` is the JSON argument object for
`record_deposit`. The full proof file contains metadata and the same object
under `contract_args`. Preview the submission command without sending it:

```sh
npm run submit -- \
  --contract native-credit.testnet \
  --args /tmp/deposit-args.json \
  --signer relayer.testnet \
  --dry-run
```

To check the signed withdrawal broadcast path without spending gas, run it
against a deployed contract after a withdrawal has been signed:

```sh
npm run broadcast -- --contract native-credit.testnet --position 1 --dry-run
```

The live withdrawal was broadcast as Sepolia tx
`0x671d78716a1bcfc79931b78d2764d340cd85e866e2df902ad1ae9e8ba0b72362`.
After finality, regenerate the account proof at the position address and submit
`confirm_withdrawal`, then verify that `pending_withdrawal` is null and
`collateral_wei` decreased by the withdrawal amount plus gas.

## 0c. Oracle checks

```sh
cd /home/uyscutty/projects/near/agents/oracle
npm test
ORACLE_KEYS_FILE="$HOME/.near-credentials/native-credit-oracle-keys.json" \
  npm run update -- --contract native-credit.testnet --eth-usd 3000 --usdc-usd 1.00 --dry-run
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

Expected: the production build succeeds. Open the dev server, connect a
MyNearWallet testnet account, and confirm that position `1` renders with
`0.05` ETH collateral, zero debt, a pending `0.01` ETH withdrawal, the live
oracle sequence, and the risk parameters. The borrow and withdraw buttons stay
disabled while the oracle is stale; repay remains available. The dashboard
labels the light-client check as real, the team oracle as partly simulated, and
test USDC as simulated.

## 1. NEAR MPC derivation

```sh
curl -sS https://rpc.testnet.near.org \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc":"2.0","id":1,"method":"query","params":{
      "request_type":"call_function","finality":"final",
      "account_id":"v1.signer-prod.testnet",
      "method_name":"derived_public_key",
      "args_base64":"eyJwYXRoIjoibmF0aXZlLWNyZWRpdDp2MToxMTE1NTExMTpwb3NpdGlvbjoxIiwicHJlZGVjZXNzb3IiOiJuYXRpdmUtY3JlZGl0LnRlc3RuZXQiLCJkb21haW5faWQiOjB9"
    }
  }'
```

Expected: a base58 secp256k1 public key. The corresponding Ethereum address is
`0xec29df4cf529cf8be4fbc8380cf732cff82389bc`.

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

Expected: thirdweb and onfinality return a proof; publicnode/tenderly/zan return
`distance to target block exceeds maximum proof window`; 1rpc reports historical
state unavailable. Re-test before relying on a provider.
