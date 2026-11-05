import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

export const METHODS = ["record_deposit", "confirm_withdrawal"];

function normalizeDeposit(value) {
  return /^\d+$/.test(value) ? `${value}NEAR` : value;
}

export function buildSubmitCommand({
  nearCli = "near",
  contract,
  method,
  argsFile,
  signer,
  network = "testnet",
  gas = "150Tgas",
  deposit = "0",
}) {
  if (!contract) throw new Error("contract is required");
  if (!METHODS.includes(method)) {
    throw new Error(`method must be one of ${METHODS.join(", ")}`);
  }
  if (!argsFile) throw new Error("args file is required");
  if (!signer) throw new Error("signer is required");
  return {
    command: nearCli,
    args: [
      "contract",
      "call-function",
      "as-transaction",
      contract,
      method,
      "file-args",
      argsFile,
      "prepaid-gas",
      gas,
      "attached-deposit",
      normalizeDeposit(deposit),
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

function usage() {
  return [
    "Usage: npm run submit -- --contract <id> --args <file> [options]",
    "",
    "Options:",
    "  --contract <id>       NEAR contract account (required)",
    "  --args <path>         contract_args JSON from `npm run proof` (required)",
    "  --signer <id>         NEAR signer account (or NEAR_SIGNER_ACCOUNT)",
    "  --method <name>       record_deposit (default) or confirm_withdrawal",
    "  --network <name>      NEAR network (default: testnet)",
    "  --gas <value>         Prepaid gas (default: 150Tgas)",
    "  --deposit <value>     Attached deposit (default: 0NEAR)",
    "  --near-cli <path>     near-cli binary (default: near)",
    "  --dry-run             Print the command without sending",
  ].join("\n");
}

async function main() {
  const { values } = parseArgs({
    options: {
      contract: { type: "string" },
      args: { type: "string" },
      signer: { type: "string" },
      method: { type: "string" },
      network: { type: "string" },
      gas: { type: "string" },
      deposit: { type: "string" },
      "near-cli": { type: "string" },
      "dry-run": { type: "boolean" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(usage());
    return;
  }
  if (!values.contract || !values.args) {
    throw new Error(usage());
  }
  if (!existsSync(values.args)) {
    throw new Error(`args file not found: ${values.args}`);
  }

  const built = buildSubmitCommand({
    nearCli: values["near-cli"] ?? "near",
    contract: values.contract,
    method: values.method ?? "record_deposit",
    argsFile: values.args,
    signer: values.signer ?? process.env.NEAR_SIGNER_ACCOUNT,
    network: values.network ?? "testnet",
    gas: values.gas ?? "150Tgas",
    deposit: values.deposit ?? "0",
  });
  const display = [built.command, ...built.args].map(shellQuote).join(" ");
  if (values["dry-run"]) {
    console.log(display);
    return;
  }

  const result = spawnSync(built.command, built.args, { stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
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
