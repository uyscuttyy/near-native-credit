import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { getPosition, nearView } from "../src/near.mjs";

async function startNear(result) {
  const calls = [];
  const server = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const payload = JSON.parse(body);
      calls.push(payload);
      response.setHeader("content-type", "application/json");
      response.end(
        JSON.stringify({
          jsonrpc: "2.0",
          id: payload.id,
          result: {
            result: [...Buffer.from(JSON.stringify(result))],
            logs: [],
            block_height: 1,
            block_hash: "test",
          },
        }),
      );
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  return {
    calls,
    server,
    url: `http://127.0.0.1:${address.port}`,
  };
}

test("nearView decodes a contract view result", async (t) => {
  const near = await startNear({ position_id: 1, collateral_wei: "42" });
  t.after(() => near.server.close());

  const result = await nearView({
    contractId: "native-credit.testnet",
    method: "get_position",
    args: { position_id: 1 },
    rpcUrl: near.url,
  });
  assert.deepEqual(result, { position_id: 1, collateral_wei: "42" });
  const params = near.calls[0].params;
  assert.equal(params.account_id, "native-credit.testnet");
  assert.equal(params.method_name, "get_position");
  assert.deepEqual(JSON.parse(Buffer.from(params.args_base64, "base64")), {
    position_id: 1,
  });
});

test("getPosition rejects a missing position", async (t) => {
  const near = await startNear(null);
  t.after(() => near.server.close());

  await assert.rejects(
    getPosition({
      contractId: "native-credit.testnet",
      positionId: 7,
      rpcUrl: near.url,
    }),
    /position 7 not found/,
  );
});
