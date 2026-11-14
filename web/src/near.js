export const CONTRACT_ID =
  import.meta.env.VITE_CONTRACT_ID ?? "v2.native-credit.testnet";
export const USDC_ID =
  import.meta.env.VITE_USDC_ID ??
  "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af";
export const NEAR_RPC =
  import.meta.env.VITE_NEAR_RPC ?? "https://rpc.testnet.near.org";

const textDecoder = new TextDecoder();

export async function view(contractId, method, args = {}) {
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "query",
    params: {
      request_type: "call_function",
      finality: "final",
      account_id: contractId,
      method_name: method,
      args_base64: btoa(
        String.fromCharCode(...new TextEncoder().encode(JSON.stringify(args))),
      ),
    },
  });
  const response = await fetch(NEAR_RPC, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body,
  });
  if (!response.ok) {
    throw new Error(`NEAR RPC ${response.status}`);
  }
  const payload = await response.json();
  if (payload.error) {
    throw new Error(payload.error.data ?? payload.error.message);
  }
  const bytes = Uint8Array.from(payload.result.result);
  return JSON.parse(textDecoder.decode(bytes));
}

export function parseUnits(value, decimals) {
  const clean = String(value).trim();
  if (!/^\d*(\.\d*)?$/.test(clean) || clean === "" || clean === ".") {
    throw new Error("Enter a valid amount");
  }
  const [whole = "0", fraction = ""] = clean.split(".");
  if (fraction.length > decimals) {
    throw new Error(`Use at most ${decimals} decimal places`);
  }
  const padded = fraction.padEnd(decimals, "0");
  return `${BigInt(whole || "0")}${padded}`.replace(/^0+(?=\d)/, "");
}

export function formatUnits(value, decimals, maximumFractionDigits = 4) {
  if (value === undefined || value === null) return "0";
  const raw = BigInt(value);
  const divisor = 10n ** BigInt(decimals);
  const whole = raw / divisor;
  const fraction = (raw % divisor).toString().padStart(decimals, "0");
  const trimmed = fraction.replace(/0+$/, "");
  const numeric = Number(`${whole}.${trimmed || "0"}`);
  return numeric.toLocaleString(undefined, {
    maximumFractionDigits,
  });
}

export function shortAddress(value, lead = 6, tail = 4) {
  if (!value || value.length <= lead + tail + 3) return value ?? "";
  return `${value.slice(0, lead)}…${value.slice(-tail)}`;
}
