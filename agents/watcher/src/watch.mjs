import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { keccak256 } from "ethereum-cryptography/keccak.js";
import {
  getConfig,
  getHealth,
  getPosition,
  getPrice,
  nearRpcUrl,
  parsePositionList,
} from "../../shared/near.mjs";
import {
  evaluatePosition,
  formatHealth,
  isPriceStale,
  priceAgeSeconds,
  summarize,
} from "../../shared/risk.mjs";
import {
  getTransactionReceipt,
  receiptAlerts,
  sepoliaRpcUrl,
  txHashToHex,
} from "./sepolia.mjs";

export function rawTxHashMatches(withdrawal) {
  if (!withdrawal?.raw_tx || !withdrawal?.tx_hash) return true;
  const hash = keccak256(Uint8Array.from(withdrawal.raw_tx));
  const expected = Uint8Array.from(withdrawal.tx_hash);
  if (hash.length !== expected.length) return false;
  return hash.every((byte, index) => byte === expected[index]);
}

export function evaluateSnapshot({
  position,
  health,
  price,
  config,
  receipt,
  receiptError,
  nowSeconds = Math.floor(Date.now() / 1000),
}) {
  const alerts = evaluatePosition({ position, health, price, config, nowSeconds });
  if (position.pending_withdrawal && !rawTxHashMatches(position.pending_withdrawal)) {
    alerts.unshift({
      severity: "critical",
      code: "raw_tx_hash_mismatch",
      message: "stored raw withdrawal transaction does not match the stored hash",
    });
  }
  if (position.pending_withdrawal?.tx_hash) {
    alerts.push(...receiptAlerts(receipt, receiptError));
  }
  return alerts;
}

export async function loadSnapshot({
  contractId,
  positionId,
  rpcUrl = nearRpcUrl(),
}) {
  const [config, price, position, health] = await Promise.all([
    getConfig({ contractId, rpcUrl }),
    getPrice({ contractId, rpcUrl }),
    getPosition({ contractId, positionId, rpcUrl }),
    getHealth({ contractId, positionId, rpcUrl }),
  ]);
  if (!position) return null;
  return { config, price, position, health };
}

export function renderReport({ positionId, position, health, alerts }) {
  const counts = summarize(alerts);
  const lines = [
    `position ${positionId} [${position.status}] health=${formatHealth(health)} alerts=${counts.critical} critical, ${counts.warning} warning`,
  ];
  for (const alert of alerts) {
    lines.push(`  [${alert.severity}] ${alert.code}: ${alert.message}`);
  }
  return lines.join("\n");
}

async function sendWebhook(url, payload) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
    signal: AbortSignal.timeout(15_000),
  });
  if (!response.ok) {
    throw new Error(`webhook HTTP ${response.status}`);
  }
}

async function runOnce({
  contractId,
  positionIds,
  rpcUrl,
  sepoliaRpc,
  webhook,
  json,
  failOnAlert,
}) {
  const results = [];
  let exitCode = 0;
  for (const positionId of positionIds) {
    try {
      const snapshot = await loadSnapshot({ contractId, positionId, rpcUrl });
      if (!snapshot) {
        if (!json) console.log(`position ${positionId}: not found`);
        continue;
      }
      let receipt;
      let receiptError;
      if (snapshot.position.pending_withdrawal?.tx_hash) {
        try {
          receipt = await getTransactionReceipt({
            txHash: snapshot.position.pending_withdrawal.tx_hash,
            rpcUrl: sepoliaRpc,
          });
        } catch (error) {
          receiptError = error.message;
        }
      }
      const alerts = evaluateSnapshot({
        ...snapshot,
        receipt,
        receiptError,
        nowSeconds: Math.floor(Date.now() / 1000),
      });
      const counts = summarize(alerts);
      const result = {
        position_id: positionId,
        owner: snapshot.position.owner,
        status: snapshot.position.status,
        health: snapshot.health,
        price_age_secs: priceAgeSeconds(snapshot.price),
        price_stale: isPriceStale(snapshot.price, snapshot.config),
        receipt_status: receipt?.status ?? null,
        counts,
        alerts,
      };
      results.push(result);
      if (!json) console.log(renderReport({ positionId, ...snapshot, alerts }));
      if (failOnAlert && (counts.critical > 0 || counts.warning > 0)) {
        exitCode = 1;
      }
    } catch (error) {
      results.push({ position_id: positionId, error: error.message });
      if (!json) console.error(`position ${positionId}: ${error.message}`);
      exitCode = 1;
    }
  }
  if (json) {
    console.log(JSON.stringify(results, null, 2));
  }
  if (webhook) {
    try {
      await sendWebhook(webhook, { contract: contractId, results });
    } catch (error) {
      console.error(`webhook failed: ${error.message}`);
      exitCode = 1;
    }
  }
  return exitCode;
}

async function main() {
  const { values } = parseArgs({
    options: {
      contract: { type: "string" },
      positions: { type: "string" },
      from: { type: "string" },
      to: { type: "string" },
      "near-rpc": { type: "string" },
      "sepolia-rpc": { type: "string" },
      interval: { type: "string" },
      loop: { type: "boolean" },
      webhook: { type: "string" },
      json: { type: "boolean" },
      "fail-on-alert": { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(
      [
        "Usage: npm run watch -- --contract <id> [options]",
        "",
        "Options:",
        "  --positions <ids>       comma-separated position ids (default 1)",
        "  --from <n> --to <n>     inclusive position range",
        "  --near-rpc <url>        NEAR RPC (or NEAR_RPC_URL)",
        "  --sepolia-rpc <url>     Sepolia RPC (or SEPOLIA_RPC_URL)",
        "  --once                  run a single scan (default)",
        "  --loop                  keep scanning",
        "  --interval <seconds>    delay between scans (default 30)",
        "  --webhook <url>         POST scan results to a webhook",
        "  --json                  print JSON",
        "  --fail-on-alert         exit 1 if any warning or critical alert exists",
      ].join("\n"),
    );
    return;
  }
  if (!values.contract) throw new Error("--contract is required");

  const positionIds = parsePositionList(
    values.positions,
    values.from ?? 0,
    values.to ?? 9,
  );
  if (positionIds.length === 0) throw new Error("no positions selected");
  const interval = Math.max(5, Number.parseInt(values.interval ?? "30", 10));
  const webhook = values.webhook ?? process.env.WATCHER_WEBHOOK_URL;

  if (!values.loop) {
    process.exitCode = await runOnce({
      contractId: values.contract,
      positionIds,
      rpcUrl: nearRpcUrl(values["near-rpc"]),
      sepoliaRpc: sepoliaRpcUrl(values["sepolia-rpc"]),
      webhook,
      json: values.json,
      failOnAlert: values["fail-on-alert"],
    });
    return;
  }

  for (;;) {
    const code = await runOnce({
      contractId: values.contract,
      positionIds,
      rpcUrl: nearRpcUrl(values["near-rpc"]),
      sepoliaRpc: sepoliaRpcUrl(values["sepolia-rpc"]),
      webhook,
      json: values.json,
      failOnAlert: values["fail-on-alert"],
    });
    if (code !== 0 && values.json) {
      console.error(`scan reported alerts (exit ${code})`);
    }
    await new Promise((resolve) => setTimeout(resolve, interval * 1000));
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
