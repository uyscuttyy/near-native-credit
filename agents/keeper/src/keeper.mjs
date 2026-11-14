import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import {
  getConfig,
  getHealth,
  getPosition,
  getPrice,
  nearRpcUrl,
  parsePositionList,
} from "../../shared/near.mjs";
import {
  estimateSeizedWei,
  formatHealth,
  isLiquidatable,
  isNearLiquidation,
  isPriceStale,
  priceAgeSeconds,
} from "../../shared/risk.mjs";

export function classifyPosition({
  positionId,
  position,
  health,
  price,
  config,
  nowSeconds = Math.floor(Date.now() / 1000),
}) {
  if (!position || health === null || health === undefined) return null;
  const debt = BigInt(position.debt_usdc ?? "0");
  const collateral = BigInt(position.collateral_wei ?? "0");
  const stale = isPriceStale(price, config, nowSeconds);
  const liquidatable = !stale && debt > 0n && isLiquidatable(health);
  const near = !stale && debt > 0n && isNearLiquidation(health);
  let estimatedSeizedWei = 0n;
  if (liquidatable && debt > 0n && price) {
    estimatedSeizedWei = estimateSeizedWei({
      repayUsdc: debt,
      usdcPrice1e8: BigInt(price.usdc_usd_1e8),
      ethPrice1e8: BigInt(price.eth_usd_1e8),
      penaltyBps: Number(config.liquidation_penalty_bps),
      collateralWei: collateral,
    });
  }
  return {
    position_id: positionId,
    owner: position.owner,
    status: position.status,
    health: String(health),
    health_label: formatHealth(health),
    debt_usdc: debt.toString(),
    collateral_wei: collateral.toString(),
    price_age_secs: priceAgeSeconds(price, nowSeconds),
    price_stale: stale,
    risk_off: Boolean(config.risk_off),
    liquidatable,
    near_liquidation: near,
    estimated_seized_wei: estimatedSeizedWei.toString(),
  };
}

export async function scanPositions({
  contractId,
  positionIds,
  rpcUrl = nearRpcUrl(),
  nowSeconds = Math.floor(Date.now() / 1000),
}) {
  const [config, price] = await Promise.all([
    getConfig({ contractId, rpcUrl }),
    getPrice({ contractId, rpcUrl }),
  ]);
  const rows = [];
  for (const positionId of positionIds) {
    try {
      const [position, health] = await Promise.all([
        getPosition({ contractId, positionId, rpcUrl }),
        getHealth({ contractId, positionId, rpcUrl }),
      ]);
      const row = classifyPosition({
        positionId,
        position,
        health,
        price,
        config,
        nowSeconds,
      });
      if (row) rows.push(row);
    } catch (error) {
      rows.push({ position_id: positionId, error: error.message });
    }
  }
  return rows;
}

export function renderTable(rows) {
  const header = [
    "position",
    "status",
    "health",
    "debt USDC",
    "price age",
    "candidate",
  ];
  const lines = [header.join("\t")];
  for (const row of rows) {
    if (row.error) {
      lines.push(`${row.position_id}\tERROR\t${row.error}`);
      continue;
    }
    const candidate = row.liquidatable
      ? "LIQUIDATABLE"
      : row.near_liquidation
        ? "near"
        : row.price_stale
          ? "stale price"
          : "no";
    lines.push(
      [
        row.position_id,
        row.status,
        row.health_label,
        row.debt_usdc,
        row.price_age_secs === null ? "—" : `${row.price_age_secs}s`,
        candidate,
      ].join("\t"),
    );
  }
  return lines.join("\n");
}

async function scanAndPrint({ contractId, positionIds, rpcUrl, json }) {
  const rows = await scanPositions({ contractId, positionIds, rpcUrl });
  if (json) {
    console.log(JSON.stringify(rows, null, 2));
  } else {
    console.log(renderTable(rows));
  }
  return rows;
}

async function main() {
  const { values } = parseArgs({
    options: {
      contract: { type: "string" },
      positions: { type: "string" },
      from: { type: "string" },
      to: { type: "string" },
      "near-rpc": { type: "string" },
      interval: { type: "string" },
      loop: { type: "boolean" },
      json: { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(
      [
        "Usage: npm run keeper -- --contract <id> [options]",
        "",
        "Options:",
        "  --positions <ids>       comma-separated position ids (default 0..9)",
        "  --from <n> --to <n>     inclusive position range",
        "  --near-rpc <url>        NEAR RPC (or NEAR_RPC_URL)",
        "  --once                  run a single scan (default)",
        "  --loop                  keep scanning",
        "  --interval <seconds>    delay between scans (default 30)",
        "  --json                  print JSON",
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
  const rpcUrl = nearRpcUrl(values["near-rpc"]);
  const interval = Math.max(5, Number.parseInt(values.interval ?? "30", 10));

  if (!values.loop) {
    await scanAndPrint({
      contractId: values.contract,
      positionIds,
      rpcUrl,
      json: values.json,
    });
    return;
  }

  for (;;) {
    await scanAndPrint({
      contractId: values.contract,
      positionIds,
      rpcUrl,
      json: values.json,
    });
    await new Promise((resolve) => setTimeout(resolve, interval * 1000));
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
