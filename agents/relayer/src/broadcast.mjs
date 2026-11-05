import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { getPosition, nearRpcUrl } from "./near.mjs";
import { resolveProviders, rpcRequest } from "./providers.mjs";

function hostOf(endpoint) {
  try {
    return new URL(endpoint).host;
  } catch {
    return endpoint;
  }
}

export function sideToHex(value) {
  if (typeof value === "string" && value.startsWith("0x")) return value;
  if (Array.isArray(value)) return `0x${Buffer.from(value).toString("hex")}`;
  throw new Error("invalid tx_hash in position");
}

export async function sendRawTransaction(providers, rawHex) {
  const failures = [];
  for (const endpoint of providers) {
    try {
      const hash = await rpcRequest(endpoint, "eth_sendRawTransaction", [
        rawHex,
      ]);
      return { endpoint, hash };
    } catch (error) {
      failures.push({ provider: hostOf(endpoint), error: error.message });
    }
  }
  throw new Error(
    `all providers rejected the transaction: ${JSON.stringify(failures)}`,
  );
}

async function main() {
  const { values } = parseArgs({
    options: {
      contract: { type: "string" },
      position: { type: "string" },
      providers: { type: "string" },
      "near-rpc": { type: "string" },
      "dry-run": { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(
      "Usage: npm run broadcast -- --contract <id> --position <id> [--dry-run]",
    );
    return;
  }
  if (!values.contract || values.position === undefined) {
    throw new Error("--contract and --position are required");
  }
  const positionId = Number.parseInt(values.position, 10);
  if (!Number.isSafeInteger(positionId) || positionId < 0) {
    throw new Error("--position must be a non-negative integer");
  }

  const position = await getPosition({
    contractId: values.contract,
    positionId,
    rpcUrl: nearRpcUrl(values["near-rpc"]),
  });
  const pending = position.pending_withdrawal;
  if (!pending || !Array.isArray(pending.raw_tx) || pending.raw_tx.length === 0) {
    throw new Error(`position ${positionId} has no signed raw transaction`);
  }
  const rawTx = Buffer.from(pending.raw_tx);
  const rawHex = `0x${rawTx.toString("hex")}`;
  const expectedHash = sideToHex(pending.tx_hash);

  if (values["dry-run"]) {
    console.log(
      JSON.stringify(
        {
          dry_run: true,
          contract: values.contract,
          position_id: positionId,
          expected_tx_hash: expectedHash,
          raw_tx: rawHex,
        },
        null,
        2,
      ),
    );
    return;
  }

  const providers = resolveProviders(values.providers);
  const sent = await sendRawTransaction(providers, rawHex);
  if (sent.hash.toLowerCase() !== expectedHash.toLowerCase()) {
    throw new Error(
      `provider returned ${sent.hash}, contract stored ${expectedHash}`,
    );
  }
  console.log(
    JSON.stringify(
      {
        contract: values.contract,
        position_id: positionId,
        tx_hash: sent.hash,
        provider: hostOf(sent.endpoint),
      },
      null,
      2,
    ),
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
