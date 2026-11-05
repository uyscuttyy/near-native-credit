import { createBlockFromRPC } from "@ethereumjs/block";
import { Common, Hardfork, Sepolia } from "@ethereumjs/common";
import { verifyMerkleProof } from "@ethereumjs/mpt";
import { RLP } from "@ethereumjs/rlp";
import { bytesToHex, hexToBytes } from "@ethereumjs/util";
import { keccak256 } from "ethereum-cryptography/keccak.js";

export const EMPTY_TRIE_ROOT =
  "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";
export const EMPTY_CODE_HASH =
  "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470";

const common = new Common({
  chain: Sepolia,
  hardfork: Hardfork.Osaka,
  eips: [7928, 7843],
});

export function normalizeAddress(value) {
  if (typeof value !== "string" || !/^0x[0-9a-fA-F]{40}$/.test(value)) {
    throw new Error(`invalid Ethereum address: ${value}`);
  }
  return value.toLowerCase();
}

export function headerRlpFromRpcBlock(rpcBlock) {
  const block = createBlockFromRPC(rpcBlock, [], {
    common,
    setHardfork: true,
  });
  return block.header.serialize();
}

export function verifyHeaderRlp(rpcBlock, headerRlp = headerRlpFromRpcBlock(rpcBlock)) {
  const computed = bytesToHex(keccak256(headerRlp));
  if (computed !== rpcBlock.hash.toLowerCase()) {
    throw new Error(
      `header hash mismatch: rpc=${rpcBlock.hash} computed=${computed}`,
    );
  }
  return headerRlp;
}

function bytesToBigInt(bytes) {
  if (bytes.length === 0) return 0n;
  return BigInt(bytesToHex(bytes));
}

function decodeAccount(value) {
  const decoded = RLP.decode(value);
  if (!Array.isArray(decoded) || decoded.length !== 4) {
    throw new Error("invalid account RLP");
  }
  const [nonce, balance, storageRoot, codeHash] = decoded;
  for (const item of decoded) {
    if (!(item instanceof Uint8Array)) {
      throw new Error("invalid account RLP field");
    }
  }
  return {
    nonce: bytesToBigInt(nonce),
    balance: bytesToBigInt(balance),
    storageRoot: bytesToHex(storageRoot),
    codeHash: bytesToHex(codeHash),
    isEoa:
      bytesToHex(codeHash) === EMPTY_CODE_HASH &&
      bytesToHex(storageRoot) === EMPTY_TRIE_ROOT,
  };
}

export async function verifyAccountProof({ stateRoot, address, proof }) {
  if (!Array.isArray(proof)) {
    throw new Error("account proof must be an array");
  }
  const normalized = normalizeAddress(address);
  const key = keccak256(hexToBytes(normalized));
  const nodes = proof.map((node) => {
    if (typeof node !== "string" || !node.startsWith("0x")) {
      throw new Error("invalid proof node");
    }
    return hexToBytes(node);
  });
  const value = await verifyMerkleProof(key, nodes, {
    root: hexToBytes(stateRoot),
  });
  if (value === null) {
    return {
      exists: false,
      nonce: 0n,
      balance: 0n,
      storageRoot: null,
      codeHash: null,
      isEoa: false,
    };
  }
  return {
    exists: true,
    ...decodeAccount(value),
  };
}

export function accountToJson(account) {
  return {
    exists: account.exists,
    nonce: account.nonce.toString(),
    balance: account.balance.toString(),
    balance_hex: `0x${account.balance.toString(16)}`,
    storage_root: account.storageRoot,
    code_hash: account.codeHash,
    is_eoa: account.isEoa,
  };
}

export function hexProofToContractArgs(proof) {
  return proof.map((node) => Array.from(hexToBytes(node)));
}

export function bytesToContractArgs(bytes) {
  return Array.from(bytes);
}
