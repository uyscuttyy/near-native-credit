import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { defaultKeysPath, loadKeys } from "./keys.mjs";
import { nearRpcUrl, nearView } from "./near.mjs";
import { signaturesForConfig, validSignatureCount } from "./sign.mjs";

const METHODS = ["update_price"];

export function buildUpdateCommand({
  nearCli = "near",
  contract,
  argsFile,
  signer,
  network = "testnet",
  gas = "100Tgas",
}) {
  return {
    command: nearCli,
    args: [
      "contract",
      "call-function",
      "as-transaction",
      contract,
      METHODS[0],
      "file-args",
      argsFile,
      "prepaid-gas",
      gas,
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

function shellQuote(value) {
  return /^[A-Za-z0-9_./:=@+-]+$/.test(value)
    ? value
    : `'${value.replaceAll("'", "'\\''")}'`;
}

function positiveInt(value, label) {
  if (!/^\d+$/.test(String(value))) throw new Error(`${label} must be an integer`);
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} must be positive`);
  }
  return parsed;
}

function usage() {
  return [
    "Usage: npm run update -- --eth-usd <price> [options]",
    "",
    "Options:",
    "  --contract <id>       Contract account (default: v2.native-credit.testnet)",
    "  --eth-usd <price>     ETH/USD price in dollars (required)",
    "  --usdc-usd <price>    USDC/USD price (default: 1.00)",
    "  --timestamp <secs>    Unix timestamp (default: now)",
    "  --sequence <n>        Price sequence (default: current + 1)",
    "  --keys <path>         Oracle key file (default: ORACLE_KEYS_FILE)",
    "  --signer <id>         NEAR signer (default: NEAR_SIGNER_ACCOUNT)",
    "  --network <name>      NEAR network (default: testnet)",
    "  --near-rpc <url>      NEAR RPC URL",
    "  --near-cli <path>     near-cli binary (default: near)",
    "  --args-out <path>     Write update_price args",
    "  --dry-run             Print the command without sending",
  ].join("\n");
}

async function main() {
  const { values } = parseArgs({
    options: {
      contract: { type: "string" },
      "eth-usd": { type: "string" },
      "usdc-usd": { type: "string" },
      timestamp: { type: "string" },
      sequence: { type: "string" },
      keys: { type: "string" },
      signer: { type: "string" },
      network: { type: "string" },
      "near-rpc": { type: "string" },
      "near-cli": { type: "string" },
      "args-out": { type: "string" },
      "dry-run": { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(usage());
    return;
  }
  if (!values["eth-usd"]) throw new Error(usage());

  const contract = values.contract ?? "v2.native-credit.testnet";
  const rpcUrl = nearRpcUrl(values["near-rpc"]);
  const config = await nearView({
    contractId: contract,
    method: "get_config",
    rpcUrl,
  });
  const current = await nearView({
    contractId: contract,
    method: "get_price",
    rpcUrl,
  });
  const sequence = values.sequence
    ? positiveInt(values.sequence, "sequence")
    : Number(current?.sequence ?? 0) + 1;
  const timestamp = values.timestamp
    ? positiveInt(values.timestamp, "timestamp")
    : Math.floor(Date.now() / 1000);
  const ethUsd = Math.round(Number(values["eth-usd"]) * 1e8);
  const usdcUsd = Math.round(Number(values["usdc-usd"] ?? "1.00") * 1e8);
  if (!Number.isSafeInteger(ethUsd) || ethUsd <= 0) {
    throw new Error("invalid --eth-usd");
  }
  if (!Number.isSafeInteger(usdcUsd) || usdcUsd <= 0) {
    throw new Error("invalid --usdc-usd");
  }

  const price = {
    eth_usd_1e8: String(ethUsd),
    usdc_usd_1e8: String(usdcUsd),
    timestamp,
    sequence,
  };
  const keys = loadKeys(values.keys ?? defaultKeysPath());
  const signatures = await signaturesForConfig(config.oracle_keys, keys, price);
  const valid = validSignatureCount(signatures);
  if (valid < config.oracle_threshold) {
    throw new Error(
      `only ${valid} signatures available, need ${config.oracle_threshold}`,
    );
  }
  const args = { price, signatures };
  const argsFile = values["args-out"] ?? "/tmp/native-credit-price.json";
  writeFileSync(argsFile, `${JSON.stringify(args, null, 2)}\n`);

  const built = buildUpdateCommand({
    nearCli: values["near-cli"] ?? "near",
    contract,
    argsFile,
    signer: values.signer ?? process.env.NEAR_SIGNER_ACCOUNT ?? contract,
    network: values.network ?? "testnet",
  });
  const display = [built.command, ...built.args].map(shellQuote).join(" ");
  if (values["dry-run"]) {
    console.log(`sequence ${sequence}, signatures ${valid}/${config.oracle_threshold}`);
    console.log(display);
    return;
  }
  const result = spawnSync(built.command, built.args, { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`near-cli exited with status ${result.status}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
