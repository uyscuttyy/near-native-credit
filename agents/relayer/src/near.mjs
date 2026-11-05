import { rpcRequest } from "./providers.mjs";

export const DEFAULT_NEAR_RPC = "https://rpc.testnet.near.org";

export function nearRpcUrl(value) {
  return value ?? process.env.NEAR_RPC_URL ?? DEFAULT_NEAR_RPC;
}

export async function nearView({
  contractId,
  method,
  args = {},
  rpcUrl = nearRpcUrl(),
}) {
  const argsBase64 = Buffer.from(JSON.stringify(args)).toString("base64");
  const result = await rpcRequest(rpcUrl, "query", {
    request_type: "call_function",
    finality: "final",
    account_id: contractId,
    method_name: method,
    args_base64: argsBase64,
  });
  if (result?.error) {
    throw new Error(`NEAR view ${method} failed: ${result.error}`);
  }
  if (!Array.isArray(result?.result)) {
    throw new Error(`NEAR view ${method} returned no result`);
  }
  const text = Buffer.from(result.result).toString("utf8");
  if (!text) {
    return null;
  }
  return JSON.parse(text);
}

export async function getPosition({
  contractId,
  positionId,
  rpcUrl = nearRpcUrl(),
}) {
  const position = await nearView({
    contractId,
    method: "get_position",
    args: { position_id: positionId },
    rpcUrl,
  });
  if (!position || typeof position !== "object") {
    throw new Error(`position ${positionId} not found`);
  }
  return position;
}
