import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { collectProof } from "../src/providers.mjs";

const BLOCK = {
  number: "0x10",
  hash: `0x${"11".repeat(32)}`,
  stateRoot: `0x${"22".repeat(32)}`,
};

function proof(balance) {
  return {
    nonce: "0x0",
    balance,
    storageHash: `0x${"33".repeat(32)}`,
    codeHash: `0x${"44".repeat(32)}`,
    accountProof: ["0x01"],
  };
}

async function startRpc(balance) {
  const server = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const payload = JSON.parse(body);
      let result;
      if (payload.method === "eth_getBlockByNumber") {
        result = BLOCK;
      } else if (payload.method === "eth_getProof") {
        result = proof(balance);
      } else {
        result = null;
      }
      response.setHeader("content-type", "application/json");
      response.end(JSON.stringify({ jsonrpc: "2.0", id: payload.id, result }));
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  return {
    server,
    url: `http://127.0.0.1:${address.port}`,
  };
}

test("collects a matching two-provider quorum", async (t) => {
  const first = await startRpc("0x64");
  const second = await startRpc("0x64");
  t.after(() => first.server.close());
  t.after(() => second.server.close());

  const result = await collectProof({
    address: "0x0000000000000000000000000000000000000001",
    providers: [first.url, second.url],
    minQuorum: 2,
  });
  assert.equal(result.snapshots.length, 2);
  assert.equal(result.signature.block_number, 16);
  assert.equal(result.signature.balance, "0x64");
});

test("rejects a provider mismatch", async (t) => {
  const first = await startRpc("0x64");
  const second = await startRpc("0x65");
  t.after(() => first.server.close());
  t.after(() => second.server.close());

  await assert.rejects(
    collectProof({
      address: "0x0000000000000000000000000000000000000001",
      providers: [first.url, second.url],
      minQuorum: 2,
    }),
    /provider quorum mismatch/,
  );
});

test("rejects insufficient quorum", async (t) => {
  const first = await startRpc("0x64");
  t.after(() => first.server.close());

  await assert.rejects(
    collectProof({
      address: "0x0000000000000000000000000000000000000001",
      providers: [first.url, "http://127.0.0.1:1"],
      minQuorum: 2,
    }),
    /only 1\/2 providers succeeded/,
  );
});
