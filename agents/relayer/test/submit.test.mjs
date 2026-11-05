import assert from "node:assert/strict";
import test from "node:test";
import { buildSubmitCommand } from "../src/submit.mjs";

test("builds a non-interactive near-cli deposit call", () => {
  const built = buildSubmitCommand({
    contract: "native-credit.testnet",
    method: "record_deposit",
    argsFile: "/tmp/deposit-args.json",
    signer: "relayer.testnet",
  });
  assert.equal(built.command, "near");
  assert.deepEqual(built.args, [
    "contract",
    "call-function",
    "as-transaction",
    "native-credit.testnet",
    "record_deposit",
    "file-args",
    "/tmp/deposit-args.json",
    "prepaid-gas",
    "150Tgas",
    "attached-deposit",
    "0NEAR",
    "sign-as",
    "relayer.testnet",
    "network-config",
    "testnet",
    "sign-with-keychain",
    "send",
  ]);
});

test("builds a withdrawal confirmation call", () => {
  const built = buildSubmitCommand({
    nearCli: "/usr/local/bin/near",
    contract: "native-credit.testnet",
    method: "confirm_withdrawal",
    argsFile: "/tmp/withdraw-args.json",
    signer: "relayer.testnet",
    network: "testnet",
    gas: "200Tgas",
    deposit: "1",
  });
  assert.equal(built.command, "/usr/local/bin/near");
  assert.equal(built.args[4], "confirm_withdrawal");
  assert.equal(built.args[8], "200Tgas");
  assert.equal(built.args[10], "1NEAR");
});

test("rejects unsupported methods", () => {
  assert.throws(
    () =>
      buildSubmitCommand({
        contract: "native-credit.testnet",
        method: "transfer",
        argsFile: "/tmp/x.json",
        signer: "relayer.testnet",
      }),
    /method must be one of/,
  );
});
