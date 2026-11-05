import { setTimeout as sleep } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { getPosition, nearRpcUrl } from "./near.mjs";

export function positionState(position) {
  const status = position.status;
  const pending = [];
  if (position.pending_deposit) pending.push("deposit");
  if (position.pending_withdrawal) pending.push("withdrawal");
  if (position.pending_borrow) pending.push("borrow");
  return {
    status,
    pending,
    collateral: position.collateral_wei?.toString?.() ?? position.collateral_wei,
    debt: position.debt_usdc?.toString?.() ?? position.debt_usdc,
    last_verified_block: position.last_verified_block,
    last_verified_nonce: position.last_verified_nonce,
  };
}

export function isSettled(position) {
  return (
    !position.pending_deposit &&
    !position.pending_withdrawal &&
    !position.pending_borrow &&
    !["Deriving", "WithdrawalPending", "LiquidationPending", "NeedsReview"].includes(
      position.status,
    )
  );
}

function stateKey(state) {
  return JSON.stringify(state);
}

async function main() {
  const { values } = parseArgs({
    options: {
      contract: { type: "string" },
      position: { type: "string" },
      "near-rpc": { type: "string" },
      interval: { type: "string" },
      timeout: { type: "string" },
      once: { type: "boolean" },
      json: { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(
      "Usage: npm run reconcile -- --contract <id> --position <id> [--once] [--timeout <seconds>]",
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
  const intervalMs = Number.parseInt(values.interval ?? "5000", 10);
  const timeoutMs = Number.parseInt(values.timeout ?? "300", 10) * 1000;
  if (intervalMs < 250 || timeoutMs < intervalMs) {
    throw new Error("invalid --interval/--timeout");
  }

  const started = Date.now();
  let lastKey;
  for (;;) {
    const position = await getPosition({
      contractId: values.contract,
      positionId,
      rpcUrl: nearRpcUrl(values["near-rpc"]),
    });
    const state = positionState(position);
    const key = stateKey(state);
    if (key !== lastKey) {
      if (values.json) {
        console.log(JSON.stringify({ position_id: positionId, ...state }));
      } else {
        console.log(
          `position ${positionId}: ${state.status}, pending=[${state.pending.join(", ")}], collateral=${state.collateral}, debt=${state.debt}, block=${state.last_verified_block}, nonce=${state.last_verified_nonce}`,
        );
      }
      lastKey = key;
    }
    if (values.once) return;
    if (position.status === "NeedsReview") {
      console.error("position requires review; stopping");
      process.exitCode = 2;
      return;
    }
    if (isSettled(position)) return;
    if (Date.now() - started >= timeoutMs) {
      throw new Error(`timed out after ${timeoutMs / 1000}s`);
    }
    await sleep(intervalMs);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
