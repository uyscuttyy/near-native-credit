import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  accountToJson,
  bytesToContractArgs,
  hexProofToContractArgs,
  normalizeAddress,
  verifyAccountProof,
} from "../src/ethereum.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const vectors = JSON.parse(
  readFileSync(
    resolve(root, "contracts/native-credit/testdata/sepolia_12058306.json"),
    "utf8",
  ),
);

function proofFor(address) {
  return vectors.accounts[address].accountProof;
}

test("verifies a real EOA account proof", async () => {
  const account = await verifyAccountProof({
    stateRoot: vectors.stateRoot,
    address: "0x0000000000000000000000000000000000000001",
    proof: proofFor("0x0000000000000000000000000000000000000001"),
  });
  assert.equal(account.exists, true);
  assert.equal(account.nonce, 0n);
  assert.equal(account.balance, 0x418c84296add147an);
  assert.equal(account.isEoa, true);
});

test("rejects a contract account as collateral", async () => {
  const account = await verifyAccountProof({
    stateRoot: vectors.stateRoot,
    address: "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238",
    proof: proofFor("0x1c7d4b196cb0c7b01d743fbc6116a902379c7238"),
  });
  assert.equal(account.exists, true);
  assert.equal(account.isEoa, false);
});

test("verifies a real absence proof", async () => {
  const account = await verifyAccountProof({
    stateRoot: vectors.stateRoot,
    address: "0xec29df4cf529cf8be4fbc8380cf732cff82389bc",
    proof: proofFor("0xec29df4cf529cf8be4fbc8380cf732cff82389bc"),
  });
  assert.equal(account.exists, false);
  assert.equal(account.balance, 0n);
});

test("rejects a tampered proof", async () => {
  const proof = structuredClone(
    proofFor("0x0000000000000000000000000000000000000001"),
  );
  const first = proof[0];
  proof[0] = `0x${first.slice(2, 4) === "00" ? "01" : "00"}${first.slice(4)}`;
  await assert.rejects(
    verifyAccountProof({
      stateRoot: vectors.stateRoot,
      address: "0x0000000000000000000000000000000000000001",
      proof,
    }),
  );
});

test("serializes contract arguments as byte arrays", () => {
  const header = new Uint8Array([2, 3, 4]);
  assert.deepEqual(bytesToContractArgs(header), [2, 3, 4]);
  assert.deepEqual(hexProofToContractArgs(["0x0102", "0x03"]), [
    [1, 2],
    [3],
  ]);
});

test("normalizes and validates addresses", () => {
  assert.equal(
    normalizeAddress("0x1C7D4B196CB0C7B01D743FBC6116A902379C7238"),
    "0x1c7d4b196cb0c7b01d743fbc6116a902379c7238",
  );
  assert.throws(() => normalizeAddress("0x1234"));
});

test("account JSON preserves decimal and hex balances", () => {
  const value = accountToJson({
    exists: true,
    nonce: 2n,
    balance: 255n,
    storageRoot: "0x00",
    codeHash: "0x01",
    isEoa: true,
  });
  assert.equal(value.balance, "255");
  assert.equal(value.balance_hex, "0xff");
  assert.equal(value.nonce, "2");
});
