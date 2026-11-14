import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import {
  getConfig,
  getHealth,
  getPosition,
  getPrice,
  getStorageBalance,
  getUsdcBalance,
  nearRpcUrl,
} from "../../shared/near.mjs";
import {
  formatHealth,
  isLiquidatable,
  isPriceStale,
  maxGasWei,
  plannedLiquidation,
} from "../../shared/risk.mjs";

function shellQuote(value) {
  return /^[A-Za-z0-9_./:=@+-]+$/.test(value)
    ? value
    : `'${value.replaceAll("'", "'\\''")}'`;
}

export function buildRegisterCommand({
  nearCli = "near",
  contractId,
  address,
  signer,
  network = "testnet",
}) {
  return {
    command: nearCli,
    args: [
      "contract",
      "call-function",
      "as-transaction",
      contractId,
      "register_sepolia_address",
      "json-args",
      JSON.stringify({ address }),
      "prepaid-gas",
      "30Tgas",
      "attached-deposit",
      "0NEAR",
      "sign-as",
      signer,
      "network-config",
      network,
      "sign-with-keychain",
      "send",
    ],
  };
}

export function buildLiquidateCommand({
  nearCli = "near",
  usdcId,
  contractId,
  positionId,
  amount,
  signer,
  network = "testnet",
  gas = "100Tgas",
}) {
  const msg = JSON.stringify({ action: "liquidate", position_id: positionId });
  const args = JSON.stringify({
    receiver_id: contractId,
    amount: String(amount),
    msg,
  });
  return {
    command: nearCli,
    args: [
      "contract",
      "call-function",
      "as-transaction",
      usdcId,
      "ft_transfer_call",
      "json-args",
      args,
      "prepaid-gas",
      gas,
      "attached-deposit",
      "1",
      "sign-as",
      signer,
      "network-config",
      network,
      "sign-with-keychain",
      "send",
    ],
  };
}

export function displayCommand(built) {
  return [built.command, ...built.args].map(shellQuote).join(" ");
}

export function planLiquidation({
  positionId,
  position,
  health,
  price,
  config,
  amount,
  balance,
  allowPartial = false,
  nowSeconds = Math.floor(Date.now() / 1000),
}) {
  if (!position) {
    return { ok: false, reason: `position ${positionId} not found` };
  }
  if (position.status !== "Idle") {
    return { ok: false, reason: `position status is ${position.status}` };
  }
  if (position.pending_deposit || position.pending_withdrawal || position.pending_borrow) {
    return { ok: false, reason: "position has a pending operation" };
  }
  if (isPriceStale(price, config, nowSeconds)) {
    return { ok: false, reason: "oracle price is stale or missing" };
  }
  if (!isLiquidatable(health)) {
    return {
      ok: false,
      reason: `health ${formatHealth(health)} is not below 1.00x`,
    };
  }
  const debt = BigInt(position.debt_usdc ?? "0");
  if (debt <= 0n) {
    return { ok: false, reason: "position has no debt" };
  }
  const requested = amount === undefined || amount === null ? debt : BigInt(amount);
  if (requested <= 0n) {
    return { ok: false, reason: "amount must be positive" };
  }
  if (requested > debt) {
    return { ok: false, reason: "amount exceeds outstanding debt" };
  }
  if (balance !== undefined && balance !== null) {
    const available = BigInt(balance);
    if (requested > available) {
      if (!allowPartial) {
        return {
          ok: false,
          reason: `USDC balance ${available} is below requested ${requested}; pass --partial to cap`,
        };
      }
      if (available <= 0n) {
        return { ok: false, reason: "USDC balance is zero" };
      }
    }
  }
  const plan = plannedLiquidation({
    debtUsdc: debt,
    requestedUsdc: requested,
    collateralWei: BigInt(position.collateral_wei),
    ethPrice1e8: BigInt(price.eth_usd_1e8),
    usdcPrice1e8: BigInt(price.usdc_usd_1e8),
    penaltyBps: Number(config.liquidation_penalty_bps),
    gasReserveWei: maxGasWei(config),
  });
  let repay = plan.repay_usdc;
  if (balance !== undefined && balance !== null && repay > BigInt(balance)) {
    if (!allowPartial) {
      return {
        ok: false,
        reason: `USDC balance ${balance} is below collateral-capped repayment ${repay}; pass --partial to cap`,
      };
    }
    repay = BigInt(balance);
  }
  if (repay <= 0n) {
    return { ok: false, reason: "collateral cannot support a liquidation repayment" };
  }
  const finalPlan = plannedLiquidation({
    debtUsdc: debt,
    requestedUsdc: repay,
    collateralWei: BigInt(position.collateral_wei),
    ethPrice1e8: BigInt(price.eth_usd_1e8),
    usdcPrice1e8: BigInt(price.usdc_usd_1e8),
    penaltyBps: Number(config.liquidation_penalty_bps),
    gasReserveWei: maxGasWei(config),
  });
  return {
    ok: true,
    position_id: positionId,
    owner: position.owner,
    health: String(health),
    health_label: formatHealth(health),
    debt_usdc: debt.toString(),
    max_repay_usdc: plan.max_repay_usdc.toString(),
    repay_usdc: repay.toString(),
    bad_debt_usdc: finalPlan.bad_debt_usdc.toString(),
    collateral_wei: position.collateral_wei,
    estimated_seized_wei: finalPlan.seized_wei.toString(),
    partial: repay < debt,
    capped_by_collateral: plan.capped_by_collateral,
  };
}

async function runCommand(built) {
  const result = spawnSync(built.command, built.args, { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`command exited with status ${result.status}`);
  }
}

async function main() {
  const { values } = parseArgs({
    options: {
      contract: { type: "string" },
      usdc: { type: "string" },
      position: { type: "string" },
      amount: { type: "string" },
      signer: { type: "string" },
      "sepolia-address": { type: "string" },
      "near-rpc": { type: "string" },
      "near-cli": { type: "string" },
      network: { type: "string" },
      partial: { type: "boolean" },
      execute: { type: "boolean" },
      json: { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(
      [
        "Usage: npm run liquidate -- --contract <id> --usdc <id> --position <n> [options]",
        "",
        "Options:",
        "  --signer <id>            liquidator NEAR account (or NEAR_SIGNER_ACCOUNT)",
        "  --amount <base units>    USDC to repay (default: full debt)",
        "  --sepolia-address <0x..> register this payout address before liquidating",
        "  --partial                cap repayment at the liquidator's USDC balance",
        "  --execute                send transactions (default: dry run)",
        "  --near-cli <path>        near-cli binary (default: near)",
        "  --network <name>         NEAR network (default: testnet)",
        "  --near-rpc <url>         NEAR RPC (or NEAR_RPC_URL)",
        "  --json                   print the plan as JSON",
      ].join("\n"),
    );
    return;
  }

  if (!values.contract || !values.usdc || values.position === undefined) {
    throw new Error("--contract, --usdc, and --position are required");
  }
  const positionId = Number.parseInt(values.position, 10);
  if (!Number.isSafeInteger(positionId) || positionId < 0) {
    throw new Error("--position must be a non-negative integer");
  }
  const signer = values.signer ?? process.env.NEAR_SIGNER_ACCOUNT;
  const rpcUrl = nearRpcUrl(values["near-rpc"]);
  const [config, price, position, health] = await Promise.all([
    getConfig({ contractId: values.contract, rpcUrl }),
    getPrice({ contractId: values.contract, rpcUrl }),
    getPosition({ contractId: values.contract, positionId, rpcUrl }),
    getHealth({ contractId: values.contract, positionId, rpcUrl }),
  ]);

  let balance = null;
  if (signer) {
    balance = await getUsdcBalance({
      usdcId: values.usdc,
      accountId: signer,
      rpcUrl,
    });
    const storage = await getStorageBalance({
      usdcId: values.usdc,
      accountId: signer,
      rpcUrl,
    });
    if (storage === null) {
      throw new Error(
        `liquidator ${signer} has no USDC storage; call storage_deposit on ${values.usdc} first`,
      );
    }
  }

  const plan = planLiquidation({
    positionId,
    position,
    health,
    price,
    config,
    amount: values.amount,
    balance,
    allowPartial: values.partial,
  });
  if (!plan.ok) {
    if (values.json) console.log(JSON.stringify(plan, null, 2));
    else console.error(`not liquidatable: ${plan.reason}`);
    process.exitCode = 2;
    return;
  }

  if (values.json && !values.execute) {
    console.log(JSON.stringify(plan, null, 2));
    return;
  }

  if (!signer) {
    throw new Error("--signer is required to build a liquidation transaction");
  }

  const registerCommand = values["sepolia-address"]
    ? buildRegisterCommand({
        nearCli: values["near-cli"],
        contractId: values.contract,
        address: values["sepolia-address"],
        signer,
        network: values.network,
      })
    : null;
  const liquidateCommand = buildLiquidateCommand({
    nearCli: values["near-cli"],
    usdcId: values.usdc,
    contractId: values.contract,
    positionId,
    amount: plan.repay_usdc,
    signer,
    network: values.network,
  });

  if (values.json) {
    console.log(
      JSON.stringify(
        { plan, register_command: registerCommand, liquidate_command: liquidateCommand },
        null,
        2,
      ),
    );
  } else {
    console.log(
      [
        `position ${positionId}: repay ${plan.repay_usdc} USDC, estimated seize ${plan.estimated_seized_wei} wei`,
        `health ${plan.health_label}, debt ${plan.debt_usdc} USDC`,
        registerCommand ? `register: ${displayCommand(registerCommand)}` : null,
        `liquidate: ${displayCommand(liquidateCommand)}`,
      ]
        .filter(Boolean)
        .join("\n"),
    );
  }

  if (!values.execute) {
    console.log("dry run: pass --execute to send");
    return;
  }

  if (registerCommand) {
    console.log("registering Sepolia payout address");
    await runCommand(registerCommand);
  }
  console.log("submitting liquidation repayment");
  await runCommand(liquidateCommand);
  console.log(
    "liquidation submitted; poll the position for ticket status, then broadcast and confirm the MPC withdrawal with agents/relayer",
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
