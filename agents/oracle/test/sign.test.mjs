import assert from "node:assert/strict";
import test from "node:test";
import { base58Decode } from "../src/keys.mjs";
import {
  priceMessage,
  signMessage,
  signaturesForConfig,
  validSignatureCount,
} from "../src/sign.mjs";

function base58Encode(bytes) {
  const alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  let number = BigInt(`0x${Buffer.from(bytes).toString("hex") || "0"}`);
  let output = "";
  while (number > 0n) {
    output = alphabet[Number(number % 58n)] + output;
    number /= 58n;
  }
  let leading = 0;
  while (leading < bytes.length && bytes[leading] === 0) leading += 1;
  return "1".repeat(leading) + output;
}

test("base58 round-trips a 32-byte seed", () => {
  const seed = Buffer.from(Array.from({ length: 32 }, (_, i) => i + 1));
  assert.deepEqual(base58Decode(base58Encode(seed)), seed);
});

test("builds the exact contract price message", () => {
  assert.equal(
    priceMessage({
      eth_usd_1e8: "250000000000",
      usdc_usd_1e8: "100000000",
      timestamp: 1700000000,
      sequence: 7,
    }).toString(),
    "native-credit:v1:price:250000000000:100000000:1700000000:7",
  );
});

test("signs each configured key in order and fills gaps", async () => {
  const first = { name: "a", publicKey: "ed25519:a", seed: Buffer.alloc(32, 1) };
  const second = { name: "b", publicKey: "ed25519:b", seed: Buffer.alloc(32, 2) };
  const price = {
    eth_usd_1e8: "1",
    usdc_usd_1e8: "1",
    timestamp: 1,
    sequence: 1,
  };
  const signatures = await signaturesForConfig(
    ["ed25519:b", "ed25519:missing", "ed25519:a"],
    [first, second],
    price,
  );
  assert.equal(signatures.length, 3);
  assert.equal(signatures[1], "");
  assert.equal(signatures[0].length, 128);
  assert.equal(signatures[2].length, 128);
  assert.notEqual(signatures[0], signatures[2]);
  assert.equal(validSignatureCount(signatures), 2);
});

test("signMessage rejects a non-32-byte seed", async () => {
  await assert.rejects(signMessage(Buffer.alloc(31), Buffer.from("x")), /32 bytes/);
});
