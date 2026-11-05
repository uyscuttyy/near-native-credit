import { writeFileSync } from "node:fs";
import { parseArgs } from "node:util";
import {
  accountToJson,
  bytesToContractArgs,
  headerRlpFromRpcBlock,
  hexProofToContractArgs,
  normalizeAddress,
  verifyAccountProof,
  verifyHeaderRlp,
} from "./ethereum.mjs";
import { DEFAULT_LIGHT_CLIENT, verifySafeBlockHash } from "./light-client.mjs";
import { collectProof, resolveProviders } from "./providers.mjs";

function hostOf(endpoint) {
  try {
    return new URL(endpoint).host;
  } catch {
    return endpoint;
  }
}

function usage() {
  return [
    "Usage: npm run proof -- --address 0x... [options]",
    "",
    "Options:",
    "  --address 0x...       Sepolia account to prove (required)",
    "  --position <id>       Add position_id to contract args",
    "  --method <name>       record_deposit (default) or confirm_withdrawal",
    "  --block <tag>         finalized (default), latest, or a block number",
    "  --providers <urls>    Comma-separated RPC URLs",
    "  --min-quorum <n>      Required successful providers (default: 2)",
    "  --light-client <id>   NEAR light client (default: client-eth2.sepolia.testnet)",
    "  --near-rpc <url>      NEAR RPC URL",
    "  --no-check-finality   Skip the light-client preflight",
    "  --out <path>          Write full JSON output to a file",
    "  --args-out <path>     Write only contract_args JSON for near-cli",
  ].join("\n");
}

async function main() {
  const { values } = parseArgs({
    options: {
      address: { type: "string" },
      position: { type: "string" },
      method: { type: "string" },
      block: { type: "string" },
      providers: { type: "string" },
      "min-quorum": { type: "string" },
      "light-client": { type: "string" },
      "near-rpc": { type: "string" },
      "no-check-finality": { type: "boolean" },
      out: { type: "string" },
      "args-out": { type: "string" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: false,
  });

  if (values.help) {
    console.log(usage());
    return;
  }
  if (!values.address) {
    throw new Error(usage());
  }

  const address = normalizeAddress(values.address);
  const providers = resolveProviders(values.providers);
  const minQuorum = Number.parseInt(values["min-quorum"] ?? "2", 10);
  if (!Number.isInteger(minQuorum) || minQuorum < 1) {
    throw new Error("--min-quorum must be a positive integer");
  }
  const method = values.method ?? "record_deposit";
  if (method !== "record_deposit" && method !== "confirm_withdrawal") {
    throw new Error("--method must be record_deposit or confirm_withdrawal");
  }
  const positionId =
    values.position === undefined
      ? undefined
      : Number.parseInt(values.position, 10);
  if (
    positionId !== undefined &&
    (!Number.isSafeInteger(positionId) || positionId < 0)
  ) {
    throw new Error("--position must be a non-negative integer");
  }

  const blockTag = values.block ?? "finalized";
  const { reference, snapshots, failures, signature } = await collectProof({
    address,
    providers,
    blockTag,
    minQuorum,
  });

  const headerRlp = verifyHeaderRlp(
    reference.block,
    headerRlpFromRpcBlock(reference.block),
  );
  if (signature.block_hash !== reference.block.hash.toLowerCase()) {
    throw new Error("reference block does not match provider quorum");
  }

  const verified = [];
  for (const snapshot of snapshots) {
    if (snapshot.block.hash.toLowerCase() !== reference.block.hash.toLowerCase()) {
      throw new Error(
        `${hostOf(snapshot.endpoint)} returned a different block hash`,
      );
    }
    const account = await verifyAccountProof({
      stateRoot: snapshot.block.stateRoot,
      address,
      proof: snapshot.proof.accountProof,
    });
    verified.push({
      provider: hostOf(snapshot.endpoint),
      endpoint: snapshot.endpoint,
      block: snapshot.block,
      proof: snapshot.proof,
      account,
    });
  }

  const accountJson = verified.map((item) => accountToJson(item.account));
  for (let index = 1; index < accountJson.length; index += 1) {
    if (JSON.stringify(accountJson[index]) !== JSON.stringify(accountJson[0])) {
      throw new Error(
        `provider account mismatch:\n${JSON.stringify(accountJson, null, 2)}`,
      );
    }
  }

  const primary =
    verified.find((item) => item.endpoint === reference.endpoint) ?? verified[0];
  if (primary.account.exists && !primary.account.isEoa) {
    throw new Error("account is a contract; Native Credit requires a plain EOA");
  }
  if (!primary.account.exists) {
    console.error(
      "warning: account is absent at this block; a deposit proof must be regenerated after funding",
    );
  }

  const lightClient = values["no-check-finality"]
    ? null
    : await verifySafeBlockHash({
        blockNumber: Number.parseInt(reference.block.number, 16),
        expectedHash: reference.block.hash,
        contractId: values["light-client"] ?? DEFAULT_LIGHT_CLIENT,
        rpcUrl: values["near-rpc"],
      });
  if (lightClient && !lightClient.matches) {
    throw new Error(
      `light client does not have ${reference.block.hash} safe at block ${lightClient.block_number}`,
    );
  }

  const contractArgs = {
    header_rlp: bytesToContractArgs(headerRlp),
    account_proof: hexProofToContractArgs(primary.proof.accountProof),
  };
  if (positionId !== undefined) {
    contractArgs.position_id = positionId;
  }

  const result = {
    network: "sepolia",
    method,
    address,
    position_id: positionId,
    block: {
      number: Number.parseInt(reference.block.number, 16),
      hash: reference.block.hash,
      state_root: reference.block.stateRoot,
      reference_provider: hostOf(reference.endpoint),
    },
    account: accountToJson(primary.account),
    quorum: {
      required: minQuorum,
      succeeded: snapshots.length,
      providers: snapshots.map((snapshot) => hostOf(snapshot.endpoint)),
      failures,
    },
    header_rlp_hex: `0x${Buffer.from(headerRlp).toString("hex")}`,
    account_proof_hex: primary.proof.accountProof,
    light_client: lightClient,
    contract_args: contractArgs,
  };

  const serialized = `${JSON.stringify(result, null, 2)}\n`;
  if (values["args-out"]) {
    writeFileSync(
      values["args-out"],
      `${JSON.stringify(contractArgs, null, 2)}\n`,
    );
    console.error(`wrote contract args ${values["args-out"]}`);
  }
  if (values.out) {
    writeFileSync(values.out, serialized);
    console.error(
      `wrote ${values.out}: block ${result.block.number}, providers ${result.quorum.providers.join(", ")}`,
    );
  } else {
    console.log(serialized.trimEnd());
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
