export const DEFAULT_PROVIDERS = [
  "https://sepolia.rpc.thirdweb.com",
  "https://eth-sepolia.api.onfinality.io/public",
];

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function providerLabel(endpoint) {
  try {
    return new URL(endpoint).host;
  } catch {
    return endpoint;
  }
}

export function resolveProviders(value) {
  const configured =
    value ??
    process.env.SEPOLIA_RPC_URLS ??
    DEFAULT_PROVIDERS.join(",");
  const providers = configured
    .split(",")
    .map((endpoint) => endpoint.trim())
    .filter(Boolean);
  if (providers.length === 0) {
    throw new Error("no Sepolia RPC providers configured");
  }
  return [...new Set(providers)];
}

export async function rpcRequest(
  endpoint,
  method,
  params,
  { timeoutMs = 20_000, retries = 2 } = {},
) {
  let lastError;
  for (let attempt = 0; attempt <= retries; attempt += 1) {
    try {
      const response = await fetch(endpoint, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
        signal: AbortSignal.timeout(timeoutMs),
      });
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      const payload = await response.json();
      if (payload.error) {
        throw new Error(JSON.stringify(payload.error));
      }
      if (payload.result === undefined) {
        throw new Error("missing JSON-RPC result");
      }
      return payload.result;
    } catch (error) {
      lastError = error;
      if (attempt < retries) {
        await sleep(400 * (attempt + 1));
      }
    }
  }
  throw new Error(
    `${providerLabel(endpoint)} ${method} failed: ${lastError?.message ?? lastError}`,
  );
}

export async function resolveReferenceBlock(providers, blockTag) {
  if (blockTag !== "finalized" && blockTag !== "latest") {
    const blockNumber = BigInt(blockTag);
    for (const endpoint of providers) {
      try {
        const block = await rpcRequest(endpoint, "eth_getBlockByNumber", [
          `0x${blockNumber.toString(16)}`,
          false,
        ]);
        if (block) return { endpoint, block };
      } catch {
        continue;
      }
    }
    throw new Error(`block ${blockTag} not available from configured providers`);
  }

  const errors = [];
  for (const endpoint of providers) {
    try {
      const block = await rpcRequest(endpoint, "eth_getBlockByNumber", [
        blockTag,
        false,
      ]);
      if (block) return { endpoint, block };
    } catch (error) {
      errors.push(error.message);
    }
  }
  throw new Error(`could not resolve ${blockTag} block: ${errors.join("; ")}`);
}

export async function fetchSnapshot(endpoint, address, blockNumber) {
  const block = await rpcRequest(endpoint, "eth_getBlockByNumber", [
    blockNumber,
    false,
  ]);
  if (!block) {
    throw new Error(`${providerLabel(endpoint)} does not have ${blockNumber}`);
  }
  const proof = await rpcRequest(endpoint, "eth_getProof", [
    address,
    [],
    blockNumber,
  ]);
  return { endpoint, block, proof };
}

function signatureOf(snapshot) {
  return {
    block_hash: snapshot.block.hash.toLowerCase(),
    block_number: Number.parseInt(snapshot.block.number, 16),
    state_root: snapshot.block.stateRoot.toLowerCase(),
    nonce: snapshot.proof.nonce,
    balance: snapshot.proof.balance,
    storage_hash: (snapshot.proof.storageHash ?? "").toLowerCase(),
    code_hash: (snapshot.proof.codeHash ?? "").toLowerCase(),
  };
}

function assertQuorum(snapshots) {
  const signatures = snapshots.map(signatureOf);
  const first = JSON.stringify(signatures[0]);
  for (let index = 1; index < signatures.length; index += 1) {
    if (JSON.stringify(signatures[index]) !== first) {
      throw new Error(
        `provider quorum mismatch:\n${JSON.stringify(signatures, null, 2)}`,
      );
    }
  }
  return signatures[0];
}

export async function collectProof({
  address,
  providers,
  blockTag = "finalized",
  minQuorum = 2,
}) {
  if (minQuorum < 1) {
    throw new Error("minQuorum must be at least 1");
  }
  const reference = await resolveReferenceBlock(providers, blockTag);
  const settled = await Promise.allSettled(
    providers.map((endpoint) =>
      fetchSnapshot(endpoint, address, reference.block.number),
    ),
  );
  const snapshots = [];
  const failures = [];
  for (let index = 0; index < settled.length; index += 1) {
    const result = settled[index];
    if (result.status === "fulfilled") {
      snapshots.push(result.value);
    } else {
      failures.push({
        provider: providerLabel(providers[index]),
        error: result.reason?.message ?? String(result.reason),
      });
    }
  }
  if (snapshots.length < minQuorum) {
    throw new Error(
      `only ${snapshots.length}/${minQuorum} providers succeeded: ${JSON.stringify(failures)}`,
    );
  }
  const signature = assertQuorum(snapshots);
  return { reference, snapshots, failures, signature };
}
