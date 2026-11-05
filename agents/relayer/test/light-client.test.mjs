import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import {
  safeBlockHash,
  verifySafeBlockHash,
} from "../src/light-client.mjs";

async function startNear(responseBytes) {
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
          result: { result: [...responseBytes], logs: [] },
        }),
      );
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return {
    calls,
    server,
    url: `http://127.0.0.1:${server.address().port}`,
  };
}

test("safeBlockHash posts Borsh u64 args and decodes the hash", async (t) => {
  const hash = "11".repeat(32);
  const near = await startNear([1, ...Buffer.from(hash, "hex")]);
  t.after(() => near.server.close());

  const result = await safeBlockHash({
    blockNumber: 12065248,
    rpcUrl: near.url,
  });
  assert.equal(result, `0x${hash}`);
  const args = Buffer.from(near.calls[0].params.args_base64, "base64");
  assert.equal(args.length, 8);
  assert.equal(args.readBigUInt64LE(0), 12065248n);
});

test("safeBlockHash returns null for an unsafe block", async (t) => {
  const near = await startNear([0]);
  t.after(() => near.server.close());

  assert.equal(await safeBlockHash({ blockNumber: 1, rpcUrl: near.url }), null);
});

test("verifySafeBlockHash reports a mismatch", async (t) => {
  const near = await startNear([1, ...Buffer.alloc(32, 0x22)]);
  t.after(() => near.server.close());

  const result = await verifySafeBlockHash({
    blockNumber: 7,
    expectedHash: `0x${"33".repeat(32)}`,
    rpcUrl: near.url,
  });
  assert.equal(result.matches, false);
  assert.equal(result.hash, `0x${"22".repeat(32)}`);
});
