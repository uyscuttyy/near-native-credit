export const DEFAULT_NEAR_RPC = "https://rpc.testnet.near.org";

export function nearRpcUrl(value) {
  return value ?? process.env.NEAR_RPC_URL ?? DEFAULT_NEAR_RPC;
}

export async function nearView({
  contractId,
  method,
  args = {},
  rpcUrl = nearRpcUrl(),
  timeoutMs = 20_000,
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
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!response.ok) {
    throw new Error(`NEAR RPC HTTP ${response.status}`);
  }
  const payload = await response.json();
  if (payload.error) {
    throw new Error(
      `NEAR RPC ${payload.error.code ?? ""} ${payload.error.message ?? ""}`.trim(),
    );
  }
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

function isMissingPosition(error) {
  return /position|panicked|does not exist|not found/i.test(error?.message ?? "");
}

export async function getConfig({ contractId, rpcUrl = nearRpcUrl() }) {
  const config = await nearView({
    contractId,
    method: "get_config",
    rpcUrl,
  });
  if (!config || typeof config !== "object") {
    throw new Error(`contract ${contractId} returned no config`);
  }
  return config;
}

export async function getPrice({ contractId, rpcUrl = nearRpcUrl() }) {
  return nearView({ contractId, method: "get_price", rpcUrl });
}

export async function getPosition({
  contractId,
  positionId,
  rpcUrl = nearRpcUrl(),
  tolerateMissing = true,
}) {
  try {
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
  } catch (error) {
    if (tolerateMissing && isMissingPosition(error)) {
      return null;
    }
    throw error;
  }
}

export async function getHealth({
  contractId,
  positionId,
  rpcUrl = nearRpcUrl(),
  tolerateMissing = true,
}) {
  try {
    return await nearView({
      contractId,
      method: "get_health",
      args: { position_id: positionId },
      rpcUrl,
    });
  } catch (error) {
    if (tolerateMissing && isMissingPosition(error)) {
      return null;
    }
    throw error;
  }
}

export async function getUsdcBalance({
  usdcId,
  accountId,
  rpcUrl = nearRpcUrl(),
}) {
  return nearView({
    contractId: usdcId,
    method: "ft_balance_of",
    args: { account_id: accountId },
    rpcUrl,
  });
}

export async function getStorageBalance({
  usdcId,
  accountId,
  rpcUrl = nearRpcUrl(),
}) {
  return nearView({
    contractId: usdcId,
    method: "storage_balance_of",
    args: { account_id: accountId },
    rpcUrl,
  });
}

export function parsePositionList(value, fallbackFrom = 0, fallbackTo = 0) {
  if (value) {
    return value
      .split(",")
      .map((item) => Number.parseInt(item.trim(), 10))
      .filter((item) => Number.isSafeInteger(item) && item >= 0);
  }
  const from = Number.parseInt(String(fallbackFrom), 10);
  const to = Number.parseInt(String(fallbackTo), 10);
  if (!Number.isSafeInteger(from) || !Number.isSafeInteger(to) || to < from) {
    return [];
  }
  return Array.from({ length: to - from + 1 }, (_, index) => from + index);
}
