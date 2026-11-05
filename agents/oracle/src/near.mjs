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
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "query",
      params: {
        request_type: "call_function",
        finality: "final",
        account_id: contractId,
        method_name: method,
        args_base64: Buffer.from(JSON.stringify(args)).toString("base64"),
      },
    }),
    signal: AbortSignal.timeout(20_000),
  });
  if (!response.ok) throw new Error(`NEAR RPC HTTP ${response.status}`);
  const payload = await response.json();
  if (payload.error) throw new Error(JSON.stringify(payload.error));
  const result = payload.result;
  if (result?.error) {
    throw new Error(`NEAR view ${method} failed: ${result.error}`);
  }
  if (!Array.isArray(result?.result)) {
    throw new Error(`NEAR view ${method} returned no result`);
  }
  const text = Buffer.from(result.result).toString("utf8");
  return text ? JSON.parse(text) : null;
}
