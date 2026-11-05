import { nearRpcUrl } from "./near.mjs";
import { rpcRequest } from "./providers.mjs";

export const DEFAULT_LIGHT_CLIENT = "client-eth2.sepolia.testnet";

function borshU64(value) {
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(BigInt(value));
  return bytes;
}

export async function safeBlockHash({
  blockNumber,
  contractId = DEFAULT_LIGHT_CLIENT,
  rpcUrl = nearRpcUrl(),
}) {
  const result = await rpcRequest(rpcUrl, "query", {
    request_type: "call_function",
    finality: "final",
    account_id: contractId,
    method_name: "block_hash_safe",
    args_base64: borshU64(blockNumber).toString("base64"),
  });
  if (result.error) {
    throw new Error(`light client error: ${result.error}`);
  }
  const bytes = Buffer.from(result.result ?? []);
  if (bytes.length === 1 && bytes[0] === 0) {
    return null;
  }
  if (bytes.length !== 33 || bytes[0] !== 1) {
    throw new Error(`unexpected block_hash_safe response: 0x${bytes.toString("hex")}`);
  }
  return `0x${bytes.subarray(1).toString("hex")}`;
}

export async function verifySafeBlockHash({
  blockNumber,
  expectedHash,
  contractId = DEFAULT_LIGHT_CLIENT,
  rpcUrl = nearRpcUrl(),
}) {
  const hash = await safeBlockHash({ blockNumber, contractId, rpcUrl });
  return {
    contract_id: contractId,
    block_number: Number(blockNumber),
    hash,
    matches: hash === expectedHash.toLowerCase(),
  };
}
