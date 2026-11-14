export const DEFAULT_SEPOLIA_RPC = "https://sepolia.rpc.thirdweb.com";

export function sepoliaRpcUrl(value) {
  return value ?? process.env.SEPOLIA_RPC_URL ?? DEFAULT_SEPOLIA_RPC;
}

export function txHashToHex(hash) {
  if (typeof hash === "string") return hash;
  if (!Array.isArray(hash) || hash.length !== 32) {
    throw new Error("withdrawal tx hash must be 32 bytes");
  }
  return `0x${Buffer.from(hash).toString("hex")}`;
}

export async function getTransactionReceipt({
  txHash,
  rpcUrl = sepoliaRpcUrl(),
  fetchImpl = fetch,
}) {
  const response = await fetchImpl(rpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_getTransactionReceipt",
      params: [txHashToHex(txHash)],
    }),
    signal: AbortSignal.timeout(20_000),
  });
  if (!response.ok) {
    throw new Error(`Sepolia RPC HTTP ${response.status}`);
  }
  const payload = await response.json();
  if (payload.error) {
    throw new Error(
      `Sepolia RPC ${payload.error.code ?? ""} ${payload.error.message ?? ""}`.trim(),
    );
  }
  return payload.result ?? null;
}

export function receiptAlerts(receipt, receiptError) {
  if (receiptError) {
    return [
      {
        severity: "warning",
        code: "receipt_lookup_failed",
        message: `could not read the Sepolia receipt: ${receiptError}`,
      },
    ];
  }
  if (receipt === null) {
    return [
      {
        severity: "warning",
        code: "receipt_missing",
        message: "signed withdrawal transaction has no Sepolia receipt yet",
      },
    ];
  }
  if (!receipt) return [];
  if (receipt.status === "0x0") {
    return [
      {
        severity: "critical",
        code: "withdrawal_failed",
        message: `Sepolia receipt status is 0x0 (gasUsed ${receipt.gasUsed ?? "unknown"})`,
      },
    ];
  }
  if (receipt.status && receipt.status !== "0x1") {
    return [
      {
        severity: "warning",
        code: "withdrawal_status_unknown",
        message: `unexpected Sepolia receipt status ${receipt.status}`,
      },
    ];
  }
  return [
    {
      severity: "info",
      code: "withdrawal_succeeded",
      message: "Sepolia receipt status is 0x1",
    },
  ];
}
