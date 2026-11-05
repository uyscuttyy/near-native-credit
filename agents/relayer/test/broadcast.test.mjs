import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { sendRawTransaction, sideToHex } from "../src/broadcast.mjs";

function startRpc(result) {
  const server = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      const payload = JSON.parse(body);
      response.setHeader("content-type", "application/json");
      response.end(
        JSON.stringify({
          jsonrpc: "2.0",
          id: payload.id,
          ...(result.error
            ? { error: result.error }
            : { result: result.value }),
        }),
      );
    });
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      resolve({
        server,
        url: `http://127.0.0.1:${server.address().port}`,
      });
    });
  });
}

test("sideToHex accepts contract byte arrays and hex strings", () => {
  assert.equal(sideToHex([0xab, 0xcd]), "0xabcd");
  assert.equal(sideToHex("0xAB"), "0xAB");
  assert.throws(() => sideToHex(1), /invalid tx_hash/);
});

test("sendRawTransaction fails over to the next provider", async (t) => {
  const bad = await startRpc({ error: { code: -32000, message: "nope" } });
  const good = await startRpc({ value: "0xabc" });
  t.after(() => bad.server.close());
  t.after(() => good.server.close());

  const sent = await sendRawTransaction([bad.url, good.url], "0x0102");
  assert.equal(sent.hash, "0xabc");
  assert.equal(sent.endpoint, good.url);
});

test("sendRawTransaction reports every provider failure", async (t) => {
  const bad = await startRpc({ error: { code: -32000, message: "nope" } });
  t.after(() => bad.server.close());

  await assert.rejects(
    sendRawTransaction([bad.url], "0x0102"),
    /all providers rejected the transaction/,
  );
});
