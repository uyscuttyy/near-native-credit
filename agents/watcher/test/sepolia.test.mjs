import test from "node:test";
import assert from "node:assert/strict";
import {
  getTransactionReceipt,
  receiptAlerts,
  txHashToHex,
} from "../src/sepolia.mjs";

test("txHashToHex renders contract byte arrays", () => {
  const hash = Array.from({ length: 32 }, (_, index) => index);
  assert.equal(
    txHashToHex(hash),
    "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
  );
  assert.equal(txHashToHex("0xabc"), "0xabc");
});

test("txHashToHex rejects malformed hashes", () => {
  assert.throws(() => txHashToHex([1, 2, 3]), /32 bytes/);
});

test("getTransactionReceipt posts eth_getTransactionReceipt", async () => {
  let request;
  const receipt = await getTransactionReceipt({
    txHash: "0xabc",
    rpcUrl: "https://example.invalid",
    fetchImpl: async (url, options) => {
      request = { url, body: JSON.parse(options.body) };
      return {
        ok: true,
        json: async () => ({ result: { status: "0x0", gasUsed: "0x5208" } }),
      };
    },
  });
  assert.equal(request.url, "https://example.invalid");
  assert.equal(request.body.method, "eth_getTransactionReceipt");
  assert.deepEqual(request.body.params, ["0xabc"]);
  assert.equal(receipt.status, "0x0");
});

test("receiptAlerts flags a reverted withdrawal", () => {
  const alerts = receiptAlerts({ status: "0x0", gasUsed: "0x5208" });
  assert.equal(alerts[0].severity, "critical");
  assert.equal(alerts[0].code, "withdrawal_failed");
});

test("receiptAlerts flags unknown and missing receipts", () => {
  assert.equal(receiptAlerts(null)[0].code, "receipt_missing");
  assert.equal(receiptAlerts("", "timeout")[0].code, "receipt_lookup_failed");
  assert.equal(
    receiptAlerts({ status: "0x2" })[0].code,
    "withdrawal_status_unknown",
  );
});

test("receiptAlerts reports a successful withdrawal as info", () => {
  const alerts = receiptAlerts({ status: "0x1" });
  assert.equal(alerts[0].severity, "info");
  assert.equal(alerts[0].code, "withdrawal_succeeded");
});
